#![cfg(feature = "integration-test")]

use anyhow::Ok;
use near_sdk::{json_types::U128, AccountId, NearToken};
use serde_json::json;
use test_utils::{
    create_test_validator, get_usdc_balance, initialize_test_token, initialize_test_vault,
    register_account_with_token, VaultViewState, MAX_COUNTER_OFFERS, VAULT_CALL_GAS,
};

#[path = "test_lock.rs"]
mod test_lock;
#[path = "test_utils.rs"]
mod test_utils;

// TODO: Cover try_add_counter_offer lock contention once we can simulate delayed callbacks in the
// sandbox.

#[tokio::test]
async fn test_counter_offer_is_accepted_and_saved() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    // Set up sandbox and accounts
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let lender = root
        .create_subaccount("lender")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;

    // Deploy validator, token, and vault
    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    // Register vault and lender with token
    register_account_with_token(&root, &token, vault.id()).await?;
    register_account_with_token(&root, &token, lender.id()).await?;

    // Mint USDC to lender  1,000 USDC (with 6 decimals)
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender.id(),
            "amount": "1000000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    // Delegate some tokens to validator from vault
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(5),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Wait 1 block
    worker.fast_forward(1).await?;

    // Open a liquidity request
    root.call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": token.id(),
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(5),
            "duration": 86400
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fetch vault state to match fields
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let request = state
        .liquidity_request
        .expect("Liquidity request not found");

    // Create a counter offer message for lender
    let msg = serde_json::json!({
        "action": "ApplyCounterOffer",
        "token": request.token,
        "amount": request.amount,
        "interest": request.interest,
        "collateral": request.collateral,
        "duration": request.duration
    })
    .to_string();

    // Lender submits a counter offer of 900_000 USDC via ft_transfer_call
    let result = lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "900000",
            "msg": msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Query vault state to verify counter offer
    let offers: serde_json::Value = vault.view("get_counter_offers").await?.json()?;

    // Expect lender's offer to exist
    let lender_id = lender.id().as_str();
    let offer = offers
        .get(lender_id)
        .expect("Expected offer from lender to be saved");

    // Expect the counter offer amount, proposer and timestamp to be correctly saved
    assert_eq!(
        offer.get("amount").unwrap().as_str().unwrap(),
        "900000",
        "Expected amount to match submitted offer"
    );
    assert_eq!(
        offer.get("proposer").unwrap().as_str().unwrap(),
        lender_id,
        "Expected proposer to be lender"
    );
    assert!(
        offer.get("timestamp").unwrap().as_u64().unwrap() > 0,
        "Expected timestamp to be recorded"
    );

    // Verify structured log
    let logs = result.logs();
    let found = logs.iter().any(|log| {
        log.contains("EVENT_JSON") && log.contains(r#""event":"counter_offer_created""#)
    });
    assert!(
        found,
        "Expected counter_offer_created log not found. Logs: {:#?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn test_counter_offer_eviction_after_max_offer() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    // Set up sandbox and accounts
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create validator, USDC token and vault
    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    // Register the vault with token contract
    register_account_with_token(&root, &token, vault.id()).await?;

    // Delegate some tokens to validator from vault
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(5),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Wait 1 block
    worker.fast_forward(1).await?;

    // Open a liquidity request
    root.call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": token.id(),
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(5),
            "duration": 86400
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Wait 1 block
    worker.fast_forward(1).await?;

    // Fetch vault state to match fields
    let vault_state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let request = vault_state
        .liquidity_request
        .expect("Liquidity request not found");

    // Create a counter offer message
    let msg = serde_json::json!({
        "action": "ApplyCounterOffer",
        "token": request.token,
        "amount": request.amount,
        "interest": request.interest,
        "collateral": request.collateral,
        "duration": request.duration
    })
    .to_string();

    // Add MAX_COUNTER_OFFERS lenders with increasing offers (100_000 to 190_000)
    let mut proposers: Vec<AccountId> = vec![];
    for i in 0..MAX_COUNTER_OFFERS {
        let lender = root
            .create_subaccount(format!("lender_{i}").as_str())
            .initial_balance(NearToken::from_near(2))
            .transact()
            .await?
            .into_result()?;

        // Add to list of proposers
        proposers.push(lender.id().clone());

        // Register the lender with token contract
        register_account_with_token(&root, &token, lender.id()).await?;

        // Transfer some USDC to lender for testing
        root.call(token.id(), "ft_transfer")
            .args_json(json!({
                "receiver_id": lender.id(),
                "amount": "1000000"
            }))
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await?
            .into_result()?;

        // Propose a counter offer by lender
        let offer_amount = 100_000 + i * 10_000;
        lender
            .call(token.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": vault.id(),
                "amount": offer_amount.to_string(),
                "msg": msg
            }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(VAULT_CALL_GAS)
            .transact()
            .await?
            .into_result()?;
    }

    // Add (MAX_COUNTER_OFFERS + 1)th lender that will propose the best offer
    let best_lender = root
        .create_subaccount("lender_best")
        .initial_balance(NearToken::from_near(2))
        .transact()
        .await?
        .into_result()?;

    // Register best_lender with token contract
    register_account_with_token(&root, &token, best_lender.id()).await?;

    // Transfer some tokens to the best_lender
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": best_lender.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    // Propose a counter offer by best_lender
    best_lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "999000",
            "msg": msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Inspect the counter offers to make sure it matches expectations
    let offers: serde_json::Value = vault.view("get_counter_offers").await?.json()?;
    assert_eq!(
        offers.as_object().unwrap().len(),
        MAX_COUNTER_OFFERS as usize,
        "Only top {} counter offers should be retained",
        MAX_COUNTER_OFFERS
    );
    assert!(
        offers.get("lender_best.test.near").is_some(),
        "Expected the best offer to be present in the map"
    );
    assert!(
        offers.get("lender_0.test.near").is_none(),
        "Expected the best offer to be present in the map"
    );
    assert!(
        vault_state.accepted_offer.is_none(),
        "Expected accepted_offer to not be set"
    );

    // Verify that lender_0.test.near who had the (MAX_COUNTER_OFFERS + 1)th (lowest) counter offer
    // Got refunded so their balance with the token contract remains the same
    let lender_0_balance = get_usdc_balance(&token, &proposers[0]).await?;
    assert!(
        lender_0_balance.0 == 1_000_000u128,
        "Expected lender_0.test.near to be refunded but balance didn't increase"
    );

    Ok(())
}

#[tokio::test]
async fn test_counter_offer_fails_when_no_request_exists() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    register_account_with_token(&root, &token, vault.id()).await?;

    let lender = root
        .create_subaccount("lender_none")
        .initial_balance(NearToken::from_near(2))
        .transact()
        .await?
        .into_result()?;
    register_account_with_token(&root, &token, lender.id()).await?;

    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let balance_before = get_usdc_balance(&token, lender.id()).await?;

    let outcome = lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "900000",
            "msg": json!({
                "action": "ApplyCounterOffer",
                "token": token.id(),
                "amount": U128(1_000_000),
                "interest": U128(100_000),
                "collateral": NearToken::from_near(5),
                "duration": 86400
            })
            .to_string()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    assert!(outcome.is_success());

    let offers: serde_json::Value = vault.view("get_counter_offers").await?.json()?;
    assert!(
        offers.is_null(),
        "Counter offers should remain empty when no request exists"
    );

    let balance_after = get_usdc_balance(&token, lender.id()).await?;
    assert_eq!(
        balance_before.0, balance_after.0,
        "Lender tokens should be refunded when request is missing"
    );

    Ok(())
}

#[tokio::test]
async fn test_counter_offer_rejects_if_not_better_than_best() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    register_account_with_token(&root, &token, vault.id()).await?;

    root.transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;

    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(5)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    worker.fast_forward(1).await?;

    root.call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": token.id(),
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(5),
            "duration": 86400
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // First lender sets the best offer at 900_000
    let lender1 = root
        .create_subaccount("lender_best")
        .initial_balance(NearToken::from_near(2))
        .transact()
        .await?
        .into_result()?;
    register_account_with_token(&root, &token, lender1.id()).await?;
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender1.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    lender1
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "900000",
            "msg": json!({
                "action": "ApplyCounterOffer",
                "token": token.id(),
                "amount": U128(1_000_000),
                "interest": U128(100_000),
                "collateral": NearToken::from_near(5),
                "duration": 86400
            })
            .to_string()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Second lender submits an inferior offer
    let lender2 = root
        .create_subaccount("lender_worse")
        .initial_balance(NearToken::from_near(2))
        .transact()
        .await?
        .into_result()?;
    register_account_with_token(&root, &token, lender2.id()).await?;
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender2.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let outcome = lender2
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "800000",
            "msg": json!({
                "action": "ApplyCounterOffer",
                "token": token.id(),
                "amount": U128(1_000_000),
                "interest": U128(100_000),
                "collateral": NearToken::from_near(5),
                "duration": 86400
            })
            .to_string()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", outcome.failures());
    assert!(
        failure_text.contains("Offer must be greater than current best offer"),
        "Expected inferior offer to be rejected, got: {failure_text}"
    );

    let balance_after = get_usdc_balance(&token, lender2.id()).await?;
    assert_eq!(balance_after.0, 1_000_000u128);

    Ok(())
}

#[tokio::test]
async fn test_counter_offer_rejects_duplicate_proposer() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    register_account_with_token(&root, &token, vault.id()).await?;
    root.transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(5)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;
    worker.fast_forward(1).await?;
    root.call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": token.id(),
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(5),
            "duration": 86400
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let lender = root
        .create_subaccount("duplicate")
        .initial_balance(NearToken::from_near(2))
        .transact()
        .await?
        .into_result()?;
    register_account_with_token(&root, &token, lender.id()).await?;
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let msg = json!({
        "action": "ApplyCounterOffer",
        "token": token.id(),
        "amount": U128(1_000_000),
        "interest": U128(100_000),
        "collateral": NearToken::from_near(5),
        "duration": 86400
    })
    .to_string();

    lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "850000",
            "msg": msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Replenish lender balance so the second attempt isn't limited by token holdings
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let second = lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "800000",
            "msg": json!({
                "action": "ApplyCounterOffer",
                "token": token.id(),
                "amount": U128(1_000_000),
                "interest": U128(100_000),
                "collateral": NearToken::from_near(5),
                "duration": 86400
            })
            .to_string()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", second.failures());
    assert!(
        failure_text.contains("Proposer already has an active offer"),
        "Expected duplicate proposer rejection, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_counter_offer_rejects_on_message_mismatch() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    register_account_with_token(&root, &token, vault.id()).await?;
    root.transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(5)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;
    worker.fast_forward(1).await?;
    root.call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": token.id(),
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(5),
            "duration": 86400
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let lender = root
        .create_subaccount("mismatch")
        .initial_balance(NearToken::from_near(2))
        .transact()
        .await?
        .into_result()?;
    register_account_with_token(&root, &token, lender.id()).await?;
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let outcome = lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "850000",
            "msg": json!({
                "action": "ApplyCounterOffer",
                "token": token.id(),
                "amount": U128(1_000_000),
                "interest": U128(123_456),
                "collateral": NearToken::from_near(5),
                "duration": 86400
            })
            .to_string()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", outcome.failures());
    assert!(
        failure_text.contains("Message fields do not match current request"),
        "Expected message mismatch failure, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_counter_offer_rejects_after_request_accepted() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;

    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    register_account_with_token(&root, &token, vault.id()).await?;
    root.transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;

    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(5)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    worker.fast_forward(1).await?;

    root.call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": token.id(),
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(5),
            "duration": 86400
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let proposer = root
        .create_subaccount("accepted_lender")
        .initial_balance(NearToken::from_near(2))
        .transact()
        .await?
        .into_result()?;
    register_account_with_token(&root, &token, proposer.id()).await?;
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": proposer.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let msg = json!({
        "action": "ApplyCounterOffer",
        "token": token.id(),
        "amount": U128(1_000_000),
        "interest": U128(100_000),
        "collateral": NearToken::from_near(5),
        "duration": 86400
    })
    .to_string();

    proposer
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "850000",
            "msg": msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    root.call(vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": proposer.id(),
            "amount": U128(850000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let other = root
        .create_subaccount("late_lender")
        .initial_balance(NearToken::from_near(2))
        .transact()
        .await?
        .into_result()?;
    register_account_with_token(&root, &token, other.id()).await?;
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": other.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let outcome = other
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "830000",
            "msg": json!({
                "action": "ApplyCounterOffer",
                "token": token.id(),
                "amount": U128(1_000_000),
                "interest": U128(100_000),
                "collateral": NearToken::from_near(5),
                "duration": 86400
            })
            .to_string()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", outcome.failures());
    assert!(
        failure_text.contains("Liquidity request already accepted"),
        "Expected rejection after acceptance, got: {failure_text}"
    );

    Ok(())
}
