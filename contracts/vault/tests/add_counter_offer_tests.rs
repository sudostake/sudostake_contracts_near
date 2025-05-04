use anyhow::Ok;
use near_sdk::{json_types::U128, AccountId, NearToken};
use serde_json::json;
use test_utils::{
    create_test_validator, get_usdc_balance, initialize_test_token, initialize_test_vault,
    register_account_with_token, VaultViewState, MAX_COUNTER_OFFERS, VAULT_CALL_GAS,
};

#[path = "test_utils.rs"]
mod test_utils;

#[tokio::test]
async fn test_counter_offer_is_accepted_and_saved() -> anyhow::Result<()> {
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
        "action": "NewCounterOffer",
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
        "action": "NewCounterOffer",
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
