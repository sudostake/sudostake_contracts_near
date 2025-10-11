#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::{json_types::U128, NearToken};
use serde_json::json;
use test_utils::{
    create_test_validator, initialize_test_vault, request_and_accept_liquidity, setup_contracts,
    setup_sandbox_and_accounts, UnstakeEntry, VaultViewState, VAULT_CALL_GAS,
};

// TODO: Cover claim_unstaked lock contention once a delayed-withdraw staking pool mock exists.

#[tokio::test]
async fn test_claim_unstaked_happy_path() -> anyhow::Result<()> {
    // Set up the sandbox environment and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Deploy and initialize the validator (staking pool)
    let validator = create_test_validator(&worker, &root).await?;

    // Deploy and initialize the vault contract
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

    // undelegate 1 NEAR from validator
    root.call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Wait 5 epochs to allow unbonding to complete
    worker.fast_forward(5 * 500).await?;

    // Call claim_unstaked to trigger withdraw_all
    let result = root
        .call(vault.id(), "claim_unstaked")
        .args_json(json!({ "validator": validator.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

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

    // Confirm entry is removed
    let entry: Option<UnstakeEntry> = vault
        .view("get_unstake_entry")
        .args_json(json!({ "validator": validator.id() }))
        .await?
        .json()?;

    assert!(
        entry.is_none(),
        "Expected unstake entry to be removed after claim_unstaked"
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_unstaked_fails_without_yocto() -> anyhow::Result<()> {
    // Set up sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create validator and initialize vault
    let validator = create_test_validator(&worker, &root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    // Delegate to validator
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(3)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast forward 1 block
    worker.fast_forward(1).await?;

    // Undelegate 1 NEAR
    root.call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Wait 5 epochs to allow unbonding to complete
    worker.fast_forward(5 * 500).await?;

    // Attempt claim_unstaked without 1 yoctoNEAR
    let result = root
        .call(vault.id(), "claim_unstaked")
        .args_json(json!({
            "validator": validator.id()
        }))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Assert failure due to missing 1 yocto
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Requires attached deposit of exactly 1 yoctoNEAR"),
        "Expected panic due to missing 1 yoctoNEAR. Got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_unstaked_fails_if_not_owner() -> anyhow::Result<()> {
    // Set up sandbox and accounts
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let alice = worker.dev_create_account().await?;

    // Create validator and initialize vault owned by root
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

    // Undelegate 1 NEAR
    root.call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast forward > 4 epochs
    worker.fast_forward(5 * 500).await?;

    // Alice tries to claim_unstaked — not allowed
    let result = alice
        .call(vault.id(), "claim_unstaked")
        .args_json(json!({
            "validator": validator.id()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Assert failure due to non-owner
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Only the vault owner can claim unstaked balance"),
        "Expected panic due to non-owner caller. Got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_unstaked_fails_if_no_entry() -> anyhow::Result<()> {
    // Set up sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Create validator and initialize vault
    let validator = create_test_validator(&worker, &root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    // Attempt to call `claim_unstaked` directly (no unstake entry exists)
    let result = root
        .call(vault.id(), "claim_unstaked")
        .args_json(json!({
            "validator": validator.id()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Assert failure due to missing unstake entry
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("No unstake entry found for validator"),
        "Expected panic due to missing unstake entry. Got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_unstaked_fails_if_epoch_not_ready() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let validator = create_test_validator(&worker, &root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    worker.fast_forward(1).await?;

    root.call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Attempt to claim immediately without waiting the unlock epochs
    let result = root
        .call(vault.id(), "claim_unstaked")
        .args_json(json!({
            "validator": validator.id()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Unstaked funds not yet claimable"),
        "Expected epoch guard failure, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_unstaked_fails_if_liquidation_active() -> anyhow::Result<()> {
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

    // Fast-forward 5 epochs to mature the unstake
    worker.fast_forward(5 * 500).await?;

    // Attempt to claim unstaked during liquidation
    let result = root
        .call(vault.id(), "claim_unstaked")
        .args_json(json!({ "validator": validator.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Check for expected panic
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Cannot claim unstaked NEAR while liquidation is in progress"),
        "Expected panic due to liquidation state. Got: {failure_text}"
    );
    Ok(())
}
