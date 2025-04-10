#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::{Gas, NearToken};
use near_workspaces::Contract;
use serde_json::json;
use test_utils::{create_test_validator, initialize_test_vault};

#[tokio::test]
async fn test_undelegate_happy_path() -> anyhow::Result<()> {
    // Initialize sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault: Contract = res.contract;

    // Fund the vault with 5 NEAR from the root
    let _ = root
        .transfer_near(vault.id(), NearToken::from_near(5))
        .await?
        .into_result()?;

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
    let found = logs.iter().any(|log| log.contains("unstake_entry_added"));
    assert!(
        found,
        "Expected 'unstake_entry_added' log not found. Logs: {:?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn test_undelegate_with_reconciliation_happy_path() -> anyhow::Result<()> {
    // Initialize sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault: Contract = res.contract;

    // Fund the vault with 5 NEAR from the root
    let _ = root
        .transfer_near(vault.id(), NearToken::from_near(5))
        .await?
        .into_result()?;

    // Call delegate for 2 NEAR to the validator
    vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(2)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

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
    let found = logs.iter().any(|log| log.contains("unstake_entry_added"));
    assert!(
        found,
        "Expected 'unstake_entry_added' log not found. Logs: {:?}",
        logs
    );

    // Wait 5 epochs to pass unbonding period
    worker.fast_forward(5).await?;

    // Call undelegate again to trigger reconciliation before new unstake
    let result = vault
        .call("undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Extract logs
    let logs = result.logs();

    // Confirm a second unstake entry was added
    let found_new_unstake = logs
        .iter()
        .filter(|log| log.contains("unstake_entry_added"))
        .count();
    assert_eq!(
        found_new_unstake, 1,
        "Expected exactly one new 'unstake_entry_added' log. Logs: {:?}",
        logs
    );

    // Confirm validator was removed
    assert!(
        logs.iter().any(|log| log.contains("validator_removed")),
        "Expected 'validator_removed' log not found. Logs: {:?}",
        logs
    );

    Ok(())
}
