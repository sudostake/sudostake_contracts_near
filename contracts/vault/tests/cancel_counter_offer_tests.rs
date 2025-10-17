#![cfg(feature = "integration-test")]

use std::collections::HashMap;

use anyhow::Ok;
use near_sdk::{json_types::U128, NearToken};
use serde_json::json;
use test_utils::{
    create_test_validator, get_usdc_balance, initialize_test_token, initialize_test_vault,
    register_account_with_token, CounterOffer, VaultViewState, VAULT_CALL_GAS,
};
use vault::types::APPLY_COUNTER_OFFER_ACTION;

#[path = "test_utils.rs"]
mod test_utils;

#[tokio::test]
async fn test_cancel_counter_offer_refunds_proposer_only() -> anyhow::Result<()> {
    // Setup sandbox and root
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

    // Mint 1_000_000 tokens to each user
    for user in [&alice, &bob, &carol] {
        root.call(token.id(), "ft_transfer")
            .args_json(json!({
                "receiver_id": user.id(),
                "amount": "1000000"
            }))
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await?
            .into_result()?;
    }

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
        "action": APPLY_COUNTER_OFFER_ACTION,
        "token": request.token,
        "amount": request.amount,
        "interest": request.interest,
        "collateral": request.collateral,
        "duration": request.duration
    })
    .to_string();

    // Each user submits a counter offer with increasing values
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

    // Alice cancels her offer
    alice
        .call(vault.id(), "cancel_counter_offer")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Check if liquidity request is still open
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_some(),
        "Liquidity request should still be open"
    );

    // Check if alice’s offer is removed, others remain
    let offers: HashMap<String, CounterOffer> = vault.view("get_counter_offers").await?.json()?;
    assert!(
        !offers.contains_key(&alice.id().to_string()),
        "Alice's offer should be cancelled"
    );
    assert!(
        offers.contains_key(&bob.id().to_string()),
        "Bob's offer should remain"
    );
    assert!(
        offers.contains_key(&carol.id().to_string()),
        "Carol's offer should remain"
    );

    // Verify that alice was refunded
    let alice_token_balance = get_usdc_balance(&token, alice.id()).await?;
    assert!(
        alice_token_balance.0 == 1_000_000u128,
        "Expected alice token balance to remain unchanged since initially funded"
    );

    Ok(())
}
