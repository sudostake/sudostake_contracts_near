#[path = "test_utils.rs"]
mod test_utils;

use near_workspaces::{network::Sandbox, Account, Worker};
use test_utils::initialize_test_vault;

#[tokio::test]
async fn test_vault_initialization() -> anyhow::Result<()> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let owner: Account = worker.root_account().unwrap();

    // Instantiate the vault contract
    let res = initialize_test_vault(&owner).await?;

    // Assert contract call succeeded
    assert!(
        res.execution_result.is_success(),
        "Contract call failed: {:?}",
        res.execution_result
    );

    // Check for emitted event log
    let logs = res.execution_result.logs();
    assert!(
        logs.iter().any(|log| log.contains("vault_created")),
        "Expected 'vault_created' log not found. Logs: {:?}",
        logs
    );
    Ok(())
}
