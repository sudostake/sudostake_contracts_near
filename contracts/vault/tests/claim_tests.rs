#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::{Gas, NearToken};
use serde_json::json;
use test_utils::{create_test_validator, initialize_test_vault};

#[tokio::test]
async fn test_claim_unstaked_happy_path() -> anyhow::Result<()> {
    // Set up the sandbox environment and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Deploy and initialize the validator (staking pool)
    let validator = create_test_validator(&worker, &root).await?;

    // Deploy and initialize the vault contract
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault = res.contract;

    // Fund the vault with 5 NEAR
    root.transfer_near(vault.id(), NearToken::from_near(5))
        .await?
        .into_result()?;

    // Delegate 2 NEAR to validator
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

    // Wait 1 epochs to allow delegate to record
    worker.fast_forward(1 * 500).await?;

    // undelegate 1 NEAR from validator
    vault
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

    // Call claim_unstaked to trigger withdraw_all + reconciliation
    let result = vault
        .call("claim_unstaked")
        .args_json(json!({ "validator": validator.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Extract logs
    let logs = result.logs();
    let found_reconciled = logs
        .iter()
        .any(|log| log.contains("unstake_entries_reconciled"));
    let found_completed = logs
        .iter()
        .any(|log| log.contains("claim_unstaked_completed"));

    // Confirm unstake_entries_reconciled
    assert!(
        found_reconciled,
        "Expected 'unstake_entries_reconciled' log not found. Logs: {:#?}",
        logs
    );

    // Confirm claim_unstaked_completed
    assert!(
        found_completed,
        "Expected 'claim_unstaked_completed' log not found. Logs: {:#?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_unstaked_partial_withdraw() -> anyhow::Result<()> {
    // Set up the sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Deploy and initialize the validator
    let validator = create_test_validator(&worker, &root).await?;

    // Deploy and initialize the vault contract
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault = res.contract;

    // Fund the vault with 5 NEAR
    root.transfer_near(vault.id(), NearToken::from_near(5))
        .await?
        .into_result()?;

    // Delegate 2 NEAR
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

    // Undelegate 0.4 NEAR (creates the first unstake entry)
    vault
        .call("undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(400_000_000_000_000_000_000_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Wait 3 epochs so the first unstake entry is almost eligible for withdrawal
    worker.fast_forward(3).await?;

    // Undelegate another 0.6 NEAR (creates a second entry still within bonding window)
    vault
        .call("undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(600_000_000_000_000_000_000_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Wait 2 more epochs — total of 5 epochs since first unstake, but only 2 for the second
    // This means only the first (0.4 NEAR) will be eligible for withdrawal
    worker.fast_forward(2).await?;

    // Call `claim_unstaked` — this will call withdraw_all + reconciliation logic
    let result = vault
        .call("claim_unstaked")
        .args_json(json!({ "validator": validator.id() }))
        .deposit(NearToken::from_yoctonear(1)) // assert_one_yocto required
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Read the logs emitted during the call
    let logs = result.logs();

    // Check that reconciliation occurred (should remove first entry)
    let found_reconciled = logs
        .iter()
        .any(|log| log.contains("unstake_entries_reconciled"));

    // Check that the claim_unstaked flow was completed
    let found_completed = logs
        .iter()
        .any(|log| log.contains("claim_unstaked_completed"));

    assert!(
        found_reconciled,
        "Expected 'unstake_entries_reconciled' log not found. Logs: {:#?}",
        logs
    );

    assert!(
        found_completed,
        "Expected 'claim_unstaked_completed' log not found. Logs: {:#?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_unstaked_works_gracefully_when_queue_is_empty() -> anyhow::Result<()> {
    // Start sandbox and get root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Deploy a fresh validator contract
    let validator = create_test_validator(&worker, &root).await?;

    // Deploy and initialize a new Vault contract instance
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault = res.contract;

    // Fund the vault with NEAR so it's able to pay for gas and storage
    root.transfer_near(vault.id(), NearToken::from_near(2))
        .await?
        .into_result()?;

    // Immediately call claim_unstaked without ever undelegating
    let result = vault
        .call("claim_unstaked")
        .args_json(json!({
            "validator": validator.id()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Check logs to confirm reconciliation and completion occurred
    let logs = result.logs();

    let found_reconciled = logs
        .iter()
        .any(|log| log.contains("unstake_entries_reconciled"));
    let found_completed = logs
        .iter()
        .any(|log| log.contains("claim_unstaked_completed"));

    assert!(
        found_reconciled,
        "Expected 'unstake_entries_reconciled' log not found. Logs: {:#?}",
        logs
    );

    assert!(
        found_completed,
        "Expected 'claim_unstaked_completed' log not found. Logs: {:#?}",
        logs
    );

    Ok(())
}
