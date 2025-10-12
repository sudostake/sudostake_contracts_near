#![cfg(feature = "integration-test")]

#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::{Gas, NearToken};
use near_workspaces::Contract;
use serde_json::json;
use test_utils::initialize_test_vault;

#[tokio::test]
async fn test_transfer_ownership_success() -> anyhow::Result<()> {
    // Setup sandbox
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let alice = worker.dev_create_account().await?;

    // Initialize vault
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault: Contract = res.contract;

    // Call transfer_ownership from the current owner to alice
    let result = vault
        .call("transfer_ownership")
        .args_json(json!({ "new_owner": alice.id() }))
        .deposit(NearToken::from_yoctonear(1)) // Required by assert_one_yocto
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;

    // Verify the event log
    let logs = result.logs();
    let found = logs.iter().any(|log| log.contains("ownership_transferred"));
    assert!(
        found,
        "Expected 'ownership_transferred' log not found. Logs: {:?}",
        logs
    );

    // Verify updated vault state
    let state: test_utils::VaultViewState = vault
        .view("get_vault_state")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(
        state.owner,
        alice.id().to_string(),
        "Vault ownership was not updated to the new owner"
    );

    Ok(())
}

#[tokio::test]
async fn test_transfer_ownership_rejects_non_owner() -> anyhow::Result<()> {
    // Setup sandbox
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let alice = worker.dev_create_account().await?;
    let bob = worker.dev_create_account().await?;

    // Deploy and initialize vault (root is the owner)
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault = res.contract;

    // alice (not the owner) tries to transfer ownership to bob
    let result = alice
        .call(vault.id(), "transfer_ownership")
        .args_json(json!({ "new_owner": bob.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;

    // Assert that the call failed
    assert!(
        result.is_failure(),
        "Expected transfer_ownership to fail when called by non-owner"
    );

    // Confirm the panic message matches the expected access control error
    let msg = result.clone().into_result().unwrap_err().to_string();
    assert!(
        msg.contains("Only the vault owner can transfer ownership"),
        "Unexpected failure message: {msg}"
    );

    Ok(())
}

#[tokio::test]
async fn test_transfer_ownership_rejects_same_owner() -> anyhow::Result<()> {
    // Set up sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Deploy and initialize the vault — root is the owner
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault = res.contract;

    // Root (owner) tries to transfer ownership to themselves
    let result = root
        .call(vault.id(), "transfer_ownership")
        .args_json(json!({ "new_owner": root.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;

    // Ensure it fails
    assert!(
        result.is_failure(),
        "Expected failure when transferring to same owner"
    );

    // Confirm the panic reason
    let msg = result.clone().into_result().unwrap_err().to_string();
    assert!(
        msg.contains("New owner must be different from the current owner"),
        "Unexpected error message: {msg}"
    );

    Ok(())
}

#[tokio::test]
async fn test_transfer_ownership_requires_1yocto() -> anyhow::Result<()> {
    // Set up sandbox and root + target account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let alice = worker.dev_create_account().await?;

    // Deploy and initialize the vault (root is owner)
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault = res.contract;

    // Attempt to call transfer_ownership without attaching 1 yoctoNEAR
    let result = root
        .call(vault.id(), "transfer_ownership")
        .args_json(json!({ "new_owner": alice.id() }))
        .gas(Gas::from_tgas(50)) // ❌ No deposit attached
        .transact()
        .await?;

    // Assert that it fails
    assert!(
        result.is_failure(),
        "Expected failure when no yoctoNEAR is attached"
    );

    // Check for assert_one_yocto failure message
    let msg = result.clone().into_result().unwrap_err().to_string();
    assert!(
        msg.contains("Requires attached deposit of exactly 1 yoctoNEAR"),
        "Unexpected failure message: {msg}"
    );

    Ok(())
}
