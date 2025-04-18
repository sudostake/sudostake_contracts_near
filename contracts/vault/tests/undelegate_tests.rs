#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::{Gas, NearToken};
use serde_json::json;
use test_utils::{create_test_validator, initialize_test_vault, UnstakeEntry};

#[tokio::test]
async fn test_undelegate_succeed() -> anyhow::Result<()> {
    // Initialize sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

    // Call delegate for 3 NEAR to the validator
    vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(3)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Fast forward 1 block so stake is visible to staking pool
    worker.fast_forward(1).await?;

    // Call undelegate for 1 NEAR from the validator
    let result = vault
        .call("undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Check for the expected unstake entry log
    let logs = result.logs();
    let found = logs.iter().any(|log| log.contains("undelegate_completed"));
    assert!(
        found,
        "Expected 'undelegate_completed' log not found. Logs: {:?}",
        logs
    );

    // Get the unstake entry for this validator
    let entry: Option<UnstakeEntry> = vault
        .view("get_unstake_entry")
        .args_json(json!({ "validator": validator.id() }))
        .await?
        .json()?;
    assert!(
        entry.is_some(),
        "Expected unstake entry to exist after undelegate"
    );

    // Assert that it was added correctly
    let unstake_entry = entry.unwrap();
    assert_eq!(unstake_entry.amount, NearToken::from_near(1).as_yoctonear());

    Ok(())
}

#[tokio::test]
async fn test_undelegate_fails_without_yocto() -> anyhow::Result<()> {
    // Initialize sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

    // Delegate 3 NEAR to validator
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(3)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Fast forward one block
    worker.fast_forward(1).await?;

    // Attempt to undelegate without 1 yoctoNEAR
    let result = root
        .call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Assert that the call failed with expected panic
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Requires attached deposit of exactly 1 yoctoNEAR"),
        "Expected failure due to missing yoctoNEAR, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_undelegate_fails_if_not_owner() -> anyhow::Result<()> {
    // Set up sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create a second account (not the owner)
    let alice = worker.dev_create_account().await?;

    // Create a test validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

    // Delegate 3 NEAR to validator
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(3)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Fast forward one block
    worker.fast_forward(1).await?;

    // Alice (not the owner) attempts to undelegate
    let result = alice
        .call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Assert that the call failed due to non-owner access
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Only the vault owner can undelegate"),
        "Expected failure due to non-owner call, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_undelegate_fails_if_validator_not_active() -> anyhow::Result<()> {
    // Set up sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

    // Attempt to call `undelegate` without ever delegating
    let result = root
        .call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Assert that the call failed due to inactive validator
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Validator is not currently active"),
        "Expected failure due to inactive validator, got: {failure_text}"
    );

    Ok(())
}
