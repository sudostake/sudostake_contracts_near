#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::{Gas, NearToken};
use near_workspaces::{network::Sandbox, Account, Contract, Worker};
use serde_json::json;
use test_utils::{create_test_validator, initialize_test_vault};

#[tokio::test]
async fn test_delegate_fast_path() -> anyhow::Result<()> {
    // Initialize sandbox environment
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let root: Account = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault: Contract = res.contract;

    // Transfer 10 NEAR from root to vault
    let _ = root
        .transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;

    // Call `delegate` with 1 NEAR and attach 1 yoctoNEAR for assert_one_yocto
    let result = vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Verify that delegate_direct was called
    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains("delegate_direct")),
        "Expected 'delegate_direct' log event to be emitted"
    );

    // Verify that validator_activated was called
    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains("validator_activated")),
        "Expected 'validator_activated' log event to be emitted"
    );

    // Verify that delegate_completed was called
    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains("delegate_completed")),
        "Expected 'delegate_completed' log event to be emitted"
    );

    Ok(())
}

#[tokio::test]
async fn test_delegate_with_reconciliation_happy_path() -> anyhow::Result<()> {
    // Set up sandbox
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault: Contract = res.contract;

    // Transfer 5 NEAR from root to vault
    let _ = root
        .transfer_near(vault.id(), NearToken::from_near(5))
        .await?
        .into_result()?;

    // Call `delegate` with 2 NEAR and attach 1 yoctoNEAR for assert_one_yocto
    let _ = vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(2)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result();

    // Initially undelegate 1 NEAR to create unstake entry
    let _ = vault
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

    // Wait 5 epochs to allow unbonding to complete
    worker.fast_forward(5 * 500).await?;

    // Now delegate again — should trigger reconciliation
    let result = vault
        .call("delegate")
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

    // Check full path was used (no delegate_direct)
    assert!(
        !logs.iter().any(|log| log.contains("delegate_direct")),
        "Expected full path, but found 'delegate_direct' log"
    );

    // Final staking log should confirm
    assert!(
        logs.iter().any(|log| log.contains("delegate_completed")),
        "Expected 'delegate_completed' log not found"
    );

    // Inspect unstake entries AFTER delegation
    let after: Vec<test_utils::UnstakeEntry> = vault
        .view("get_unstake_entries")
        .args_json(json!({ "validator": validator.id() }))
        .await?
        .json()?;

    assert_eq!(
        after.len(),
        0,
        "Expected unstake entries to be cleared after reconciliation"
    );

    Ok(())
}
