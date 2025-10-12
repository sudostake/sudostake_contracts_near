#![cfg(feature = "integration-test")]

use std::collections::HashMap;

use anyhow::Ok;
use near_sdk::{json_types::U128, AccountId, NearToken};
use serde_json::json;

use test_utils::{
    create_test_validator, get_usdc_balance, initialize_test_token, initialize_test_vault,
    register_account_with_token, CounterOffer, VaultViewState, VAULT_CALL_GAS,
};

#[path = "test_utils.rs"]
mod test_utils;
#[path = "test_lock.rs"]
mod test_lock;

#[tokio::test]
async fn test_accept_counter_offer_succeeds_and_refunds_others() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    // Setup sandbox
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create users
    let alice = root
        .create_subaccount("alice")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;
    let bob = root
        .create_subaccount("bob")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;
    let carol = root
        .create_subaccount("carol")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;

    // Deploy validator, token, and vault
    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    // Register vault and users with the token
    for account in [vault.id(), alice.id(), bob.id(), carol.id()] {
        register_account_with_token(&root, &token, account).await?;
    }

    // Mint tokens to each user
    for user in [&alice, &bob, &carol] {
        root.call(token.id(), "ft_transfer")
            .args_json(json!({ "receiver_id": user.id(), "amount": "1000000" }))
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await?
            .into_result()?;
    }

    // Delegate from vault to validator to pass collateral check
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

    // Fast-forward to let stake finalize
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

    // Fetch request details for offer message
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let request = state
        .liquidity_request
        .expect("Liquidity request not found");

    // Create a counter offer message for lender
    let msg = serde_json::json!({
        "action": "NewCounterOffer",
        "token": request.token,
        "amount": request.amount,
        "interest": request.interest,
        "collateral": request.collateral,
        "duration": request.duration
    })
    .to_string();

    // Each user submits a counter offer
    let offer_amounts = vec![800_000, 850_000, 900_000];
    for (user, amount) in [&alice, &bob, &carol].iter().zip(offer_amounts) {
        user.call(token.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": vault.id(),
                "amount": amount.to_string(),
                "msg": msg,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(VAULT_CALL_GAS)
            .transact()
            .await?
            .into_result()?;
    }

    // Vault owner accepts carol's offer (highest)
    let result = root
        .call(vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": carol.id(),
            "amount": U128(900_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Extract and inspect logs
    let logs = result.logs();
    let found = logs.iter().any(|log| {
        log.contains("EVENT_JSON")
            && log.contains(r#""event":"counter_offer_accepted""#)
            && log.contains(r#""accepted_proposer":"carol.test.near""#)
    });
    assert!(found, "Log should mention accepted proposer: {:#?}", logs);

    // Verify accepted offer is set correctly
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let accepted = state.accepted_offer.expect("Accepted offer should exist");
    assert!(accepted.get("lender").unwrap() == carol.id().as_str());

    // Ensure counter_offers field is cleared
    let offers: Option<HashMap<String, CounterOffer>> =
        vault.view("get_counter_offers").await?.json()?;
    assert!(
        offers.is_none() || offers.as_ref().unwrap().is_empty(),
        "Expected counter offers to be cleared"
    );

    // Ensure refunds were issued to alice and bob
    let alice_balance = get_usdc_balance(&token, alice.id()).await?;
    let bob_balance = get_usdc_balance(&token, bob.id()).await?;
    assert_eq!(alice_balance.0, 1_000_000, "Alice should be refunded");
    assert_eq!(bob_balance.0, 1_000_000, "Bob should be refunded");

    Ok(())
}

#[tokio::test]
async fn test_accept_counter_offer_requires_yocto() -> anyhow::Result<()> {
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
        .create_subaccount("yocto_lender")
        .initial_balance(NearToken::from_near(2))
        .transact()
        .await?
        .into_result()?;
    register_account_with_token(&root, &token, lender.id()).await?;
    root.call(token.id(), "ft_transfer")
        .args_json(json!({ "receiver_id": lender.id(), "amount": "1000000" }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "850000",
            "msg": json!({
                "action": "NewCounterOffer",
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

    let outcome = root
        .call(vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": lender.id(),
            "amount": U128(850000)
        }))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", outcome.failures());
    assert!(
        failure_text.contains("Requires attached deposit of exactly 1 yoctoNEAR"),
        "Expected yocto guard failure, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_accept_counter_offer_rejects_non_owner() -> anyhow::Result<()> {
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
        .create_subaccount("non_owner_lender")
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

    lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "820000",
            "msg": json!({
                "action": "NewCounterOffer",
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

    let alice = worker.dev_create_account().await?;
    let outcome = alice
        .call(vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": lender.id(),
            "amount": U128(820000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", outcome.failures());
    assert!(
        failure_text.contains("Only the vault owner can accept a counter offer"),
        "Expected owner guard failure, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_accept_counter_offer_rejects_missing_proposer() -> anyhow::Result<()> {
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
        .create_subaccount("actual_lender")
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
    lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "870000",
            "msg": json!({
                "action": "NewCounterOffer",
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

    let fake_proposer: AccountId = "fake.near".parse().unwrap();
    let outcome = root
        .call(vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": fake_proposer,
            "amount": U128(870000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", outcome.failures());
    assert!(
        failure_text.contains("Counter offer from proposer not found"),
        "Expected missing proposer failure, got: {failure_text}"
    );

    // Verify original offer still present
    let offers: serde_json::Value = vault.view("get_counter_offers").await?.json()?;
    assert!(offers.get(&lender.id().to_string()).is_some());

    Ok(())
}

#[tokio::test]
async fn test_accept_counter_offer_rejects_amount_mismatch() -> anyhow::Result<()> {
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
        .create_subaccount("amount_lender")
        .initial_balance(NearToken::from_near(2))
        .transact()
        .await?
        .into_result()?;
    register_account_with_token(&root, &token, proposer.id()).await?;
    root.call(token.id(), "ft_transfer")
        .args_json(json!({ "receiver_id": proposer.id(), "amount": "1000000" }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;
    proposer
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "860000",
            "msg": json!({
                "action": "NewCounterOffer",
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

    let outcome = root
        .call(vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": proposer.id(),
            "amount": U128(850000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;


    let failure_text = format!("{:?}", outcome.failures());
    assert!(
        failure_text.contains("Provided amount does not match the counter offer"),
        "Expected amount mismatch failure, got: {failure_text}"
    );

    let balance_after = get_usdc_balance(&token, proposer.id()).await?;
    let vault_balance = get_usdc_balance(&token, vault.id()).await?;
    // Proposer initially had 1_000_000 and staked 860_000 as a counter offer; on amount mismatch,
    // accept_counter_offer fails and no refund occurs, so balance remains 140_000.
    assert_eq!(balance_after.0, 140_000u128);

    Ok(())
}
