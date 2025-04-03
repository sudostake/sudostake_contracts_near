use near_sdk::NearToken;
use near_workspaces::{self as workspaces, network::Sandbox, Account, Contract, Worker};
use serde_json::json;

const FACTORY_WASM_PATH: &str = "../../res/factory.wasm";

#[tokio::test]
async fn test_factory_initialization() -> anyhow::Result<()> {
    let worker: Worker<Sandbox> = workspaces::sandbox().await?;
    let owner: Account = worker.root_account().unwrap();

    let wasm = std::fs::read(FACTORY_WASM_PATH)?;
    let factory: Contract = owner.deploy(&wasm).await?.into_result()?;

    let res = factory
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "vault_minting_fee": NearToken::from_near(1),
        }))
        .transact()
        .await?;

    assert!(res.is_success(), "Contract call failed: {:?}", res);
    Ok(())
}
