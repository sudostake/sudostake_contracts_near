#[path = "test_utils.rs"]
mod test_utils;

use near_sdk::{Gas, NearToken};
use near_workspaces::{network::Sandbox, Account, Contract, Worker};
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
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault: Contract = res.contract;

    // Transfer 10 NEAR from root to vault
    root.transfer_near(vault.id(), NearToken::from_near(10))
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
