use near_sdk::{Gas, NearToken};
use near_workspaces::{self as workspaces, network::Sandbox, Account, Contract, Worker};
use serde_json::json;

const FACTORY_WASM_PATH: &str = "../../res/factory.wasm";
const VAULT_WASM_PATH: &str = "../../res/vault.wasm";

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

#[tokio::test]
async fn test_set_vault_code() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let owner = worker.root_account()?;

    // Deploy factory contract
    let wasm = std::fs::read(FACTORY_WASM_PATH)?;
    let factory: Contract = owner.deploy(&wasm).await?.into_result()?;

    // Initialize factory
    let _ = factory
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "vault_minting_fee": NearToken::from_near(1),
        }))
        .transact()
        .await?;

    // Read vault WASM
    let vault_wasm = std::fs::read(VAULT_WASM_PATH)?;

    // Call set_vault_code
    let result = factory
        .call("set_vault_code")
        .args_json(json!({
            "code": vault_wasm
        }))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    assert!(
        result.is_success(),
        "Vault code upload failed: {:?}",
        result
    );
    Ok(())
}

#[tokio::test]
async fn test_set_vault_creation_fee() -> anyhow::Result<()> {
    use near_workspaces::types::NearToken;

    let worker = workspaces::sandbox().await?;
    let owner = worker.root_account()?;

    // Deploy factory contract
    let wasm = std::fs::read(FACTORY_WASM_PATH)?;
    let factory: Contract = owner.deploy(&wasm).await?.into_result()?;

    // Initialize factory contract
    factory
        .call("new")
        .args_json(serde_json::json!({
            "owner": owner.id(),
            "vault_minting_fee": NearToken::from_near(1),
        }))
        .transact()
        .await?
        .into_result()?;

    // Call set_vault_creation_fee
    let _ = factory
        .call("set_vault_creation_fee")
        .args_json(serde_json::json!({
            "new_fee": NearToken::from_near(2),
        }))
        .transact()
        .await?
        .into_result()?;

    Ok(())
}
