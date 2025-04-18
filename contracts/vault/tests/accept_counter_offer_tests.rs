use std::collections::HashMap;

use anyhow::Ok;
use near_sdk::{json_types::U128, NearToken};
use serde_json::json;
use test_utils::{
    create_test_validator, get_usdc_balance, initialize_test_token, initialize_test_vault,
    register_account_with_token, CounterOffer, VaultViewState, VAULT_CALL_GAS,
};

#[path = "test_utils.rs"]
mod test_utils;

#[tokio::test]
async fn test_accept_counter_offer_succeeds_and_refunds_others() -> anyhow::Result<()> {
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
