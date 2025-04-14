use anyhow::Ok;
use near_sdk::{json_types::U128, NearToken};
use serde_json::json;
use test_utils::{create_test_validator, initialize_test_vault, VaultViewState, VAULT_CALL_GAS};
#[path = "test_utils.rs"]
mod test_utils;

#[tokio::test]
async fn test_request_liquidity_finalizes_if_enough_stake() -> anyhow::Result<()> {
    // Initialize sandbox environment
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Deploy and initialize the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

    // Fund the vault with 10 NEAR
    root.transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;

    // Delegate 5 NEAR to the validator
    let _ = vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(5),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast forward 1 block so stake is visible to staking pool
    worker.fast_forward(1).await?;

    // Call request_liquidity with 5 NEAR collateral
    let result = vault
        .call("request_liquidity")
        .args_json(json!({
            "token": "usdc.token.near",
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(5),
            "duration": 60 * 60 * 24
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
            && log.contains(r#""event":"liquidity_request_opened""#)
            && log.contains(r#""token":"usdc.token.near""#)
            && log.contains(r#""amount":"1000000""#)
            && log.contains(r#""interest":"100000""#)
            && log.contains(r#""collateral":"5000000000000000000000000""#)
            && log.contains(r#""duration":86400"#)
    });

    // Assert that the structured log was correctly emitted by the vault
    assert!(
        found,
        "Expected liquidity_request_opened log not found in logs: {:#?}",
        logs
    );

    // Fetch and verify contract state
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_some(),
        "Expected liquidity_request to be present"
    );
    assert!(
        state.pending_liquidity_request.is_none(),
        "Expected pending_liquidity_request to be cleared"
    );

    Ok(())
}

#[tokio::test]
async fn test_request_liquidity_fails_if_total_stake_insufficient() -> anyhow::Result<()> {
    // Initialize sandbox environment
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Deploy and initialize the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

    // Fund the vault with 10 NEAR
    root.transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;

    // Delegate 2 NEAR (insufficient stake)
    let _ = vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(2),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast forward to ensure stake is visible
    worker.fast_forward(1).await?;

    // Try requesting liquidity with 5 NEAR collateral
    let result = vault
        .call("request_liquidity")
        .args_json(json!({
            "token": "usdc.token.near",
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(5), // more than what was staked
            "duration": 60 * 60 * 24
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Expect failure
    assert!(
        result.is_failure(),
        "Expected failure when staked amount is less than collateral, but call succeeded"
    );

    // Check logs for warning
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Insufficient staked NEAR to satisfy requested collateral"),
        "Expected error message not found. Got: {failure_text}"
    );

    Ok(())
}
