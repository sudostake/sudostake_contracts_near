#![cfg(feature = "integration-test")]

#[path = "test_utils.rs"]
mod test_utils;

use crate::test_utils::{
    get_usdc_balance, initialize_test_token, register_account_with_token,
    transfer_tokens_to_vault,
};
use near_sdk::{json_types::U128, NearToken};
use serde_json::json;
use test_utils::{
    initialize_test_vault_on_sub_account, request_and_accept_liquidity, setup_contracts,
    setup_sandbox_and_accounts, withdraw_ft, VaultViewState, VAULT_CALL_GAS,
};

#[tokio::test]
async fn vault_receives_ft_logs_event() -> anyhow::Result<()> {
    // Initialize sandbox environment
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Deploy the vault contract and initialize it
    let vault = initialize_test_vault_on_sub_account(&root).await?.contract;

    // Deploy and initialize the fungible token contract (USDC mock)
    let token = initialize_test_token(&root).await?;

    // Register the vault on the token contract so it can receive tokens
    register_account_with_token(&root, &token, &vault.id()).await?;

    // Transfer tokens to the vault using `ft_transfer_call`
    let amount = 100_000_000; // 100 USDC (6 decimals)
    let msg = "test-message";
    let result = transfer_tokens_to_vault(&root, &token, &vault, amount, msg)
        .await?
        .into_result()?;

    // Extract logs from the transaction and search for the ft_on_transfer event
    let logs = result.logs();
    let found = logs.iter().any(|log| {
        log.contains("EVENT_JSON")
            && log.contains(r#""event":"ft_on_transfer""#)
            && log.contains(&format!(r#""sender":"{}""#, root.id()))
            && log.contains(&format!(r#""amount":"{}""#, amount))
            && log.contains(&format!(r#""msg":"{}""#, msg))
    });

    // Assert that the structured log was correctly emitted by the vault
    assert!(
        found,
        "Expected ft_on_transfer log not found in logs: {:#?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn withdraw_near_emits_event() -> anyhow::Result<()> {
    // Initialize sandbox environment
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Deploy and initialize the vault contract
    let res = initialize_test_vault_on_sub_account(&root).await?;
    let vault = res.contract;

    // Send 1 NEAR to the vault so it has balance to withdraw
    let _ = root
        .transfer_near(vault.id(), near_sdk::NearToken::from_near(1))
        .await?;

    // Call withdraw_balance to withdraw NEAR back to root
    let amount = near_sdk::NearToken::from_near(1);
    let result = root
        .call(vault.id(), "withdraw_balance")
        .args_json(serde_json::json!({
            "token_address": null,
            "amount": amount.as_yoctonear().to_string(),
            "to": root.id()
        }))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Extract logs from the transaction and search for the withdraw_near event
    let logs = result.logs();
    let found = logs.iter().any(|log| {
        log.contains("EVENT_JSON")
            && log.contains(r#""event":"withdraw_near""#)
            && log.contains(&format!(r#""to":"{}""#, root.id()))
            && log.contains(&format!(r#""amount":"{}""#, amount.as_yoctonear()))
    });

    // Assert that the structured log was correctly emitted by the vault
    assert!(
        found,
        "Expected withdraw_near log not found in logs: {:#?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn withdraw_ft_emits_event() -> anyhow::Result<()> {
    // Initialize sandbox environment
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create a new test account: alice
    let alice = root
        .create_subaccount("alice")
        .initial_balance(near_sdk::NearToken::from_near(5))
        .transact()
        .await?
        .into_result()?;

    // Deploy and initialize the vault contract
    let vault = initialize_test_vault_on_sub_account(&root).await?.contract;

    // Deploy and initialize the token
    let token = initialize_test_token(&root).await?;

    // Register the vault and alice with the token contract
    register_account_with_token(&root, &token, &vault.id()).await?;
    register_account_with_token(&root, &token, &alice.id()).await?;

    // Transfer 100 USDC to the vault via direct `ft_transfer`
    let amount = 100_000_000; // 100 USDC (6 decimals)
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": amount.to_string()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    // Withdraw from vault to alice
    let result = withdraw_ft(&vault, &token, &root, &alice, amount).await?;

    // Extract and inspect logs
    let logs = result.logs();
    let found = logs.iter().any(|log| {
        log.contains("EVENT_JSON")
            && log.contains(r#""event":"withdraw_ft""#)
            && log.contains(&format!(r#""to":"{}""#, alice.id()))
            && log.contains(&format!(r#""amount":"{}""#, amount))
            && log.contains(&format!(r#""token":"{}""#, token.id()))
    });

    // Assert that the structured log was correctly emitted by the vault
    assert!(
        found,
        "Expected withdraw_ft log not found in logs: {:#?}",
        logs
    );

    // Allow the asynchronous transfer to settle before querying balances.
    worker.fast_forward(1).await?;

    // Query for Alice balance on the token contract
    // Query for Alice balance on the token contract using helper
    let alice_balance = get_usdc_balance(&token, alice.id()).await?;

    // Assert alice received the tokens
    assert_eq!(alice_balance.0, amount);

    Ok(())
}

#[tokio::test]
async fn non_owner_cannot_withdraw_near() -> anyhow::Result<()> {
    // Initialize sandbox environment
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create a non-owner user (bob)
    let bob = root
        .create_subaccount("bob")
        .initial_balance(near_sdk::NearToken::from_near(5))
        .transact()
        .await?
        .into_result()?;

    // Deploy the vault (owned by root)
    let vault = initialize_test_vault_on_sub_account(&root).await?.contract;

    // Fund the vault with 1 NEAR
    let _ = root
        .transfer_near(vault.id(), near_sdk::NearToken::from_near(1))
        .await?;

    // Let bob attempt to withdraw
    let result = bob
        .call(vault.id(), "withdraw_balance")
        .args_json(serde_json::json!({
            "token_address": null,
            "amount": near_sdk::NearToken::from_near(1).as_yoctonear().to_string(),
            "to": bob.id()
        }))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Assert that it fails
    assert!(
        result.is_failure(),
        "Expected failure when non-owner(bob) tries to withdraw_balance"
    );

    // Check for "Only the vault owner can withdraw"
    let msg = result.clone().into_result().unwrap_err().to_string();
    assert!(
        msg.contains("Only the vault owner can withdraw"),
        "Unexpected failure message: {msg}"
    );

    Ok(())
}

#[tokio::test]
async fn test_near_withdrawal_fails_during_liquidation() -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;

    // Setup contracts
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    // Query the vault's available balance
    let available: U128 = vault.view("view_available_balance").await?.json()?;
    let available_yocto = available.0;

    // Compute how much to delegate (leave 2 NEAR for repayment)
    let leave_behind = NearToken::from_near(2).as_yoctonear();
    let to_delegate = available_yocto - leave_behind;
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(to_delegate)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward to simulate validator update
    worker.fast_forward(1).await?;

    // Request and accept liquidity request
    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    // Patch accepted_at to simulate expiration
    vault
        .call("set_accepted_offer_timestamp")
        .args_json(json!({ "timestamp": 1_000_000_000 }))
        .transact()
        .await?
        .into_result()?;

    // Call process_claims — should use 2 NEAR, unstake remaining 3 NEAR
    lender
        .call(vault.id(), "process_claims")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Check vault state — loan should still be active
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_some(),
        "Liquidity request should still be open"
    );
    assert!(
        state.accepted_offer.is_some(),
        "Accepted offer should still be active"
    );

    // Transfer some tokens to the vault
    root.transfer_near(vault.id(), near_sdk::NearToken::from_near(10))
        .await?
        .into_result()?;

    // Try withdrawing while liquidation is active
    let amount = near_sdk::NearToken::from_near(1);
    let result = root
        .call(vault.id(), "withdraw_balance")
        .args_json(serde_json::json!({
            "token_address": null,
            "amount": amount.as_yoctonear().to_string(),
            "to": root.id()
        }))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Assert the withdrawing fails with liquidation error
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Cannot withdraw NEAR while liquidation is in progress"),
        "Expected failure due to liquidation, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_nep_token_withdrawal_fails_during_counter_offers() -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;

    // Setup contracts
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    // Delegate some tokens to validator
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(10)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward to simulate validator update
    worker.fast_forward(1).await?;

    // vault owner requests liquidity
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

    // Try withdrawing token balance while the request is open for counter offers
    let amount = 100_000;
    let result = withdraw_ft(&vault, &token, &root, &lender, amount).await?;

    // Assert the withdrawing fails with pending counter offers error
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Cannot withdraw requested token while counter offers are pending"),
        "Expected failure due to pending counter offers, got: {failure_text}"
    );

    Ok(())
}
