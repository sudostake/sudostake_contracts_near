#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::{Gas, NearToken};
use serde_json::json;
use test_utils::{create_test_validator, initialize_test_vault, UnstakeEntry};

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
    let found_completed = logs
        .iter()
        .any(|log| log.contains("claim_unstaked_completed"));

    // Confirm claim_unstaked_completed
    assert!(
        found_completed,
        "Expected 'claim_unstaked_completed' log not found. Logs: {:#?}",
        logs
    );

    // Query unstake entries after claim
    let entries: Vec<UnstakeEntry> = vault
        .call("get_unstake_entries")
        .args_json((validator.id(),))
        .view()
        .await?
        .json()?;

    // Confirm that the list is empty
    assert!(
        entries.is_empty(),
        "Expected unstake entries to be cleared, found: {:?}",
        entries
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

    // Wait 5 epochs to allow unbonding to complete
    worker.fast_forward(5 * 500).await?;

    // Undelegate 0.6 NEAR (creates the second unstake entry)
    // Which automatically withdraw_all the available 0.4NEAR
    // that is ready, before unstaking the new 0.6NEAR
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

    // Call `claim_unstaked` — this will call withdraw_all + reconciliation logic
    // This should not do anything as the 0.4NEAR is already claimed
    let result = vault
        .call("claim_unstaked")
        .args_json(json!({ "validator": validator.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Read the logs emitted during the call
    let logs = result.logs();

    // Check that the claim_unstaked flow was completed
    let found_completed = logs
        .iter()
        .any(|log| log.contains("claim_unstaked_completed"));
    assert!(
        found_completed,
        "Expected 'claim_unstaked_completed' log not found. Logs: {:#?}",
        logs
    );

    // Validate remaining unstake entry
    let actual_entries: Vec<UnstakeEntry> = vault
        .call("get_unstake_entries")
        .args_json((validator.id(),))
        .view()
        .await?
        .json()?;

    // Expect exactly one entry with 0.6 NEAR remaining
    assert_eq!(actual_entries.len(), 1, "Expected 1 entry remaining");
    let expected_entry = UnstakeEntry {
        amount: 600_000_000_000_000_000_000_000,
        epoch_height: actual_entries[0].epoch_height,
    };
    assert_eq!(
        actual_entries[0], expected_entry,
        "Remaining entry does not match expected"
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
    let found_completed = logs
        .iter()
        .any(|log| log.contains("claim_unstaked_completed"));

    assert!(
        found_completed,
        "Expected 'claim_unstaked_completed' log not found. Logs: {:#?}",
        logs
    );

    Ok(())
}
