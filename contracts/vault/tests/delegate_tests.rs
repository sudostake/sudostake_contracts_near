#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::{json_types::U128, Gas, NearToken};
use near_workspaces::{network::Sandbox, Account, Worker};
use serde_json::json;
use test_utils::{
    create_named_test_validator, create_test_validator, initialize_test_vault,
    request_and_accept_liquidity, setup_contracts, setup_sandbox_and_accounts, VaultViewState,
    MAX_ACTIVE_VALIDATORS, VAULT_CALL_GAS,
};

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

#[tokio::test]
async fn test_delegate_fails_if_liquidation_active() -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;

    // Setup contracts
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    // Query the vault's available balance
    let available: U128 = vault.view("view_available_balance").await?.json()?;
    let available_yocto = available.0;

    // Compute how much to delegate (leave 2 NEAR for repayment)
    let leave_behind = NearToken::from_near(2).as_yoctonear();
    let to_delegate = available_yocto - leave_behind;
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(to_delegate)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward to simulate validator update
    worker.fast_forward(1).await?;

    // Request and accept liquidity request
    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    // Patch accepted_at to simulate expiration
    vault
        .call("set_accepted_offer_timestamp")
        .args_json(json!({ "timestamp": 1_000_000_000 }))
        .transact()
        .await?
        .into_result()?;

    // Call process_claims — should use 2 NEAR, unstake remaining 3 NEAR
    lender
        .call(vault.id(), "process_claims")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Check vault state — loan should still be active
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_some(),
        "Liquidity request should still be open"
    );
    assert!(
        state.accepted_offer.is_some(),
        "Accepted offer should still be active"
    );

    // Transfer some tokens to the vault
    root.transfer_near(vault.id(), near_sdk::NearToken::from_near(10))
        .await?
        .into_result()?;

    // Try delegating while liquidation is active
    let result = root
        .call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Assert the delegation fails with liquidation error
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Cannot delegate while liquidation is in progress"),
        "Expected failure due to liquidation, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_delegate_fails_when_max_validators_reached() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create distinct validators to fill the active set
    let mut validators = Vec::with_capacity(MAX_ACTIVE_VALIDATORS + 1);
    for i in 0..=MAX_ACTIVE_VALIDATORS {
        let name = format!("validator-limit-{}", i);
        validators.push(create_named_test_validator(&worker, &root, &name).await?);
    }

    let vault = initialize_test_vault(&root).await?.contract;

    // Delegate to each validator up to the limit
    for validator in validators.iter().take(MAX_ACTIVE_VALIDATORS) {
        root.call(vault.id(), "delegate")
            .args_json(json!({
                "validator": validator.id(),
                "amount": NearToken::from_near(1),
            }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(VAULT_CALL_GAS)
            .transact()
            .await?
            .into_result()?;
    }

    // Next delegation to a new validator should fail due to cap
    let overflow = validators.last().unwrap();
    let result = root
        .call(vault.id(), "delegate")
        .args_json(json!({
            "validator": overflow.id(),
            "amount": NearToken::from_near(1),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("You can only stake with"),
        "Expected validator cap failure, got: {failure_text}"
    );

    Ok(())
}
