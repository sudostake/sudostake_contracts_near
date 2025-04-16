#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::{Gas, NearToken};
use near_workspaces::{network::Sandbox, Account, Worker};
use serde_json::json;
use test_utils::{create_test_validator, initialize_test_vault};

#[tokio::test]
async fn test_delegate_succeed() -> anyhow::Result<()> {
    // Initialize sandbox environment
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let root: Account = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

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

    // Verify that delegate_completed was called
    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains("delegate_completed")),
        "Expected 'delegate_completed' log event to be emitted"
    );

    // Fetch active validators via view method
    let validators: Vec<String> = vault.view("get_active_validators").await?.json()?;

    // Confirm the validator is now in the active set
    assert!(
        validators.contains(&validator.id().to_string()),
        "Validator should be added to active_validators set"
    );

    Ok(())
}

#[tokio::test]
async fn test_delegate_fails_without_yocto() -> anyhow::Result<()> {
    // Set up sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create a test validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

    // Attempt to call `delegate` WITHOUT attaching 1 yoctoNEAR
    let result = vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Assert that the transaction failed with the expected panic
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Requires attached deposit of exactly 1 yoctoNEAR"),
        "Expected failure due to missing yoctoNEAR deposit, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_delegate_fails_if_not_owner() -> anyhow::Result<()> {
    // Set up sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create a second user (not the vault owner)
    let alice = worker.dev_create_account().await?;

    // Create a test validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

    // Have alice attempt to call `delegate`
    let result = alice
        .call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Assert the call failed due to non-owner access
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Only the vault owner can delegate stake"),
        "Expected failure due to non-owner delegation, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_delegate_fails_if_amount_zero() -> anyhow::Result<()> {
    // Set up sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create a test validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

    // Attempt to delegate 0 NEAR
    let result = root
        .call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(0)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Assert the call failed due to zero amount
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Amount must be greater than 0"),
        "Expected failure due to zero delegation amount, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_delegate_fails_if_insufficient_balance() -> anyhow::Result<()> {
    // Set up sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create a test validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let vault = initialize_test_vault(&root).await?.contract;

    // Get total vault balance
    let vault_balance = vault.view_account().await?.balance;

    // Attempt to delegate entire vault_balance
    // This should fail because of the STORAGE_BUFFER (0.01 NEAR reserved)
    let result = root
        .call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": vault_balance
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Assert failure due to available balance being insufficient
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("exceeds available balance"),
        "Expected failure due to insufficient balance, got: {failure_text}"
    );

    Ok(())
}
