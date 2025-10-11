use anyhow::Ok;
use near_sdk::{json_types::U128, NearToken};
use serde_json::json;
use test_utils::{
    create_named_test_validator, create_test_validator, initialize_test_vault, VaultViewState,
    VAULT_CALL_GAS,
};
#[path = "test_utils.rs"]
mod test_utils;

// TODO: Add integration coverage for request_liquidity lock contention when staking-pool callbacks
// can be delayed in the sandbox.

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

    Ok(())
}

#[tokio::test]
async fn test_request_liquidity_fails_without_yocto() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let vault = initialize_test_vault(&root).await?.contract;

    let result = root
        .call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": "usdc.token.near",
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(1),
            "duration": 60 * 60 * 24,
        }))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Requires attached deposit of exactly 1 yoctoNEAR"),
        "Expected deposit guard failure, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_request_liquidity_fails_if_not_owner() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let alice = worker.dev_create_account().await?;
    let vault = initialize_test_vault(&root).await?.contract;

    let result = alice
        .call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": "usdc.token.near",
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(1),
            "duration": 60 * 60 * 24,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Only the vault owner can request liquidity"),
        "Expected owner guard failure, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_request_liquidity_fails_if_collateral_zero() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let vault = initialize_test_vault(&root).await?.contract;

    let result = root
        .call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": "usdc.token.near",
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_yoctonear(0),
            "duration": 60 * 60 * 24,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Collateral must be positive"),
        "Expected collateral guard failure, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_request_liquidity_fails_if_amount_zero() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let vault = initialize_test_vault(&root).await?.contract;

    let result = root
        .call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": "usdc.token.near",
            "amount": U128(0),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(1),
            "duration": 60 * 60 * 24,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Requested amount must be greater than zero"),
        "Expected amount guard failure, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_request_liquidity_fails_if_duration_zero() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let vault = initialize_test_vault(&root).await?.contract;

    let result = root
        .call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": "usdc.token.near",
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(1),
            "duration": 0,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Duration must be non-zero"),
        "Expected duration guard failure, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_request_liquidity_fails_if_request_already_open() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let validator = create_test_validator(&worker, &root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

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
            "token": "usdc.token.near",
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(5),
            "duration": 60 * 60 * 24,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let result = root
        .call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": "usdc.token.near",
            "amount": U128(500_000),
            "interest": U128(50_000),
            "collateral": NearToken::from_near(2),
            "duration": 60 * 60 * 12,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("A request is already open"),
        "Expected subsequent request to fail, got: {failure_text}"
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

    // Delegate 2 NEAR (insufficient for 5 NEAR collateral)
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
        .await?
        .into_result()?;

    // Extract logs from the callback phase
    let logs = result.logs();

    // Check that the logs include the expected failure event
    let found = logs
        .iter()
        .any(|log| log.contains("liquidity_request_failed_insufficient_stake"));
    assert!(
        found,
        "Expected log 'liquidity_request_failed_insufficient_stake' not found in logs: {:?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn test_request_liquidity_prunes_zero_stake_validators() -> anyhow::Result<()> {
    // Initialize sandbox environment
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize two test validators
    let validator_1 = create_named_test_validator(&worker, &root, "validator_1").await?;
    let validator_2 = create_named_test_validator(&worker, &root, "validator_2").await?;

    // Deploy and initialize the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

    // Delegate 5 NEAR only to validator_1
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator_1.id(),
            "amount": NearToken::from_near(5),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast forward
    worker.fast_forward(1).await?;

    // Delegate 5 NEAR only to validator_2
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator_2.id(),
            "amount": NearToken::from_near(5),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast forward
    worker.fast_forward(1).await?;

    // Get total staked at validator_2 and undelegate all
    let staked: U128 = validator_2
        .view("get_account_staked_balance")
        .args_json(json!({ "account_id": vault.id() }))
        .await?
        .json()?;

    root.call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator_2.id(),
            "amount": staked.0.to_string(),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Wait 5 epochs to allow unbonding to complete
    worker.fast_forward(5 * 500).await?;

    // Claim all unstaked balance from validator_2
    root.call(vault.id(), "claim_unstaked")
        .args_json(json!({ "validator": validator_2.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast forward
    worker.fast_forward(1).await?;

    // Try requesting liquidity with the remaining staked 5 NEAR as collateral
    let result = root
        .call(vault.id(), "request_liquidity")
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

    // Inspect logs for liquidity_request_opened event
    let logs = result.logs();
    let found = logs
        .iter()
        .any(|log| log.contains("liquidity_request_opened"));
    assert!(
        found,
        "Expected 'liquidity_request_opened' log not found. Logs: {:?}",
        logs
    );

    // Fetch active validators after pruning logic
    let validators: Vec<String> = vault.view("get_active_validators").await?.json()?;

    // Confirm that validator_2 has been removed
    assert!(
        !validators.contains(&validator_2.id().to_string()),
        "Expected validator_2 to be pruned, but it is still present: {:?}",
        validators
    );

    Ok(())
}
