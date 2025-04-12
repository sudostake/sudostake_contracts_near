#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::json_types::U128;
use test_utils::{initialize_test_vault_on_sub_account, withdraw_ft, VAULT_CALL_GAS};

use crate::test_utils::{
    initialize_test_token, register_account_with_token, transfer_tokens_to_vault,
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

    // Transfer 100 USDC to the vault
    let amount = 100_000_000;
    let msg = "init";
    let _ = transfer_tokens_to_vault(&root, &token, &vault, amount, msg).await?;

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

    // Query for Alice balance on the token contract
    let alice_balance: U128 = token
        .call("ft_balance_of")
        .args_json(serde_json::json!({ "account_id": alice.id() }))
        .view()
        .await?
        .json::<U128>()?;

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
