#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::{json_types::U128, Gas, NearToken};
use serde_json::json;
use test_utils::{
    create_test_validator, initialize_test_token, initialize_test_vault,
    register_account_with_token, UnstakeEntry, VaultViewState, VAULT_CALL_GAS,
};

// TODO: Add integration coverage for undelegate lock contention once a mock validator that
// delays callbacks is available.

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

    // Fetch current active validators
    let active_validators: Vec<String> = vault
        .view("get_active_validators")
        .await?
        .json()
        .expect("Failed to decode active validators");

    // Assert validator is still in the active set
    assert!(
        active_validators.contains(&validator.id().to_string()),
        "Validator should still remain in the active set"
    );

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

#[tokio::test]
async fn test_undelegate_fails_if_amount_zero() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let validator = create_test_validator(&worker, &root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    // Delegate some stake so validator becomes active
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    worker.fast_forward(1).await?;

    // Attempt to undelegate zero NEAR
    let result = root
        .call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(0)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Amount must be greater than 0"),
        "Expected zero-amount guard, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_undelegate_fails_if_offer_already_accepted() -> anyhow::Result<()> {
    // Set up sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create lender account
    let lender = root
        .create_subaccount("lender")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;

    // Deploy contracts
    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    // Register accounts with token
    for account in [vault.id(), lender.id()] {
        register_account_with_token(&root, &token, account).await?;
    }

    // Fund lender
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    // Delegate from vault to validator
    root.call(vault.id(), "delegate")
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

    // Vault owner opens liquidity request
    root.call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": token.id(),
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(5),
            "duration": 86400
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fetch the request for offer matching
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let request = state.liquidity_request.unwrap();
    let msg = serde_json::json!({
        "action": "NewCounterOffer",
        "token": request.token,
        "amount": request.amount,
        "interest": request.interest,
        "collateral": request.collateral,
        "duration": request.duration
    })
    .to_string();

    // Lender submits counter offer
    lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "900000",
            "msg": msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Vault owner accepts lender's offer
    root.call(vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": lender.id(),
            "amount": U128(900_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Try to undelegate after accepting — should fail
    let result = root
        .call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure = format!("{:?}", result.failures());
    assert!(
        failure.contains("Cannot undelegate when a liquidity request is open"),
        "Expected undelegate to fail after offer acceptance, got: {failure}"
    );

    Ok(())
}

#[tokio::test]
async fn test_validator_removed_after_unstake_to_zero() -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let validator = create_test_validator(&worker, &root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    // Delegate 2 NEAR to validator
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(2)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast forward 1 block
    worker.fast_forward(1).await?;

    // Undelegate the full 2 NEAR
    root.call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(2)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast forward 1 block
    worker.fast_forward(1).await?;

    // Fetch current active validators
    let active_validators: Vec<String> = vault
        .view("get_active_validators")
        .await?
        .json()
        .expect("Failed to decode active validators");

    // Assert validator is no longer in the active set
    assert!(
        !active_validators.contains(&validator.id().to_string()),
        "Validator should be removed after unstaking to zero"
    );

    Ok(())
}
