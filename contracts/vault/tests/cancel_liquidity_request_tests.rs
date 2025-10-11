#![cfg(feature = "integration-test")]

use near_sdk::{json_types::U128, NearToken};
use near_workspaces::{network::Sandbox, Worker};
use serde_json::json;
use test_utils::{
    create_test_validator, initialize_test_token, InstantiateTestVaultResult, VaultViewState,
    VAULT_CALL_GAS,
};
#[path = "test_utils.rs"]
mod test_utils;

#[tokio::test]
async fn cancel_liquidity_request_clears_state() -> anyhow::Result<()> {
    // Spin up sandbox + root account
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Deploy vault contract under a subaccount
    let InstantiateTestVaultResult {
        contract: vault, ..
    } = test_utils::initialize_test_vault_on_sub_account(&root).await?;

    // Deploy a mock USDC token contract
    let usdc = initialize_test_token(&root).await?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Fund the vault with NEAR so it can delegate
    root.transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;

    // Delegate 5 NEAR to the validator
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

    // Call `request_liquidity` from vault owner
    root.call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": usdc.id(),
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

    // Call `cancel_liquidity_request` from vault owner
    root.call(vault.id(), "cancel_liquidity_request")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Call view method to get vault state
    let state: VaultViewState = vault
        .view("get_vault_state")
        .args_json(json!({}))
        .await?
        .json()?;

    // Assert that liquidity request is cleared
    assert!(
        state.liquidity_request.is_none(),
        "Expected liquidity_request to be None"
    );

    Ok(())
}
