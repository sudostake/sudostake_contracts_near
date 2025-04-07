use near_workspaces::{network::Sandbox, Account, Contract, Worker};
use serde_json::json;
const VAULT_WASM_PATH: &str = "../../res/vault.wasm";

#[tokio::test]
async fn test_vault_initialization() -> anyhow::Result<()> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let owner: Account = worker.root_account().unwrap();

    let wasm = std::fs::read(VAULT_WASM_PATH)?;
    let vault: Contract = owner.deploy(&wasm).await?.into_result()?;

    let res = vault
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "index": 0,
            "version": 1
        }))
        .transact()
        .await?;

    // Assert contract call succeeded
    assert!(res.is_success(), "Contract call failed: {:?}", res);

    // Check for emitted event log
    let logs = res.logs();
    assert!(
        logs.iter().any(|log| log.contains("vault_created")),
        "Expected 'vault_created' log not found. Logs: {:?}",
        logs
    );
    Ok(())
}
