use near_sdk::{Gas, NearToken};
use near_workspaces::types::CryptoHash;
use near_workspaces::{self as workspaces, network::Sandbox, Account, Contract, Worker};
use serde_json::json;

const FACTORY_WASM_PATH: &str = "../../res/factory.wasm";
const VAULT_WASM_PATH: &str = "../../res/vault.wasm";

// NEAR costs (yoctoNEAR)
const STORAGE_COST_PER_BYTE: u128 = 100_000_000_000_000_000_000; // 0.0001 NEAR
const STORAGE_BUFFER: u128 = 10_000_000_000_000_000_000_000; // 0.01 NEAR

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

#[tokio::test]
async fn test_mint_vault_success() -> anyhow::Result<()> {
    // Initialize sandbox and root account
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let owner: Account = worker.root_account()?;

    // Deploy factory contract
    let factory_wasm = std::fs::read(FACTORY_WASM_PATH)?;
    let factory: Contract = owner.deploy(&factory_wasm).await?.into_result()?;

    // Load vault.wasm and calculate required minting fee
    let vault_wasm = std::fs::read(VAULT_WASM_PATH)?;
    let wasm_bytes = vault_wasm.len() as u128;
    let deploy_cost = wasm_bytes * STORAGE_COST_PER_BYTE;
    let total_fee_yocto = deploy_cost + STORAGE_BUFFER;
    let minting_fee = NearToken::from_yoctonear(total_fee_yocto);

    // Initialize factory with calculated minting fee
    factory
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "vault_minting_fee": minting_fee
        }))
        .transact()
        .await?
        .into_result()?;

    // Upload vault code
    factory
        .call("set_vault_code")
        .args_json(json!({ "code": vault_wasm }))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Create a user account
    let user = owner
        .create_subaccount("user")
        .initial_balance(NearToken::from_near(50))
        .transact()
        .await?
        .into_result()?;

    // Call mint_vault with the correct fee
    let result = user
        .call(factory.id(), "mint_vault")
        .deposit(minting_fee.clone())
        .max_gas()
        .transact()
        .await?;

    assert!(result.is_success(), "mint_vault call failed: {:?}", result);

    // Check if vault subaccount was created and contract deployed
    let vault_id = format!("vault-0.{}", factory.id()).parse()?;
    let vault_account = worker.view_account(&vault_id).await?;

    assert_ne!(
        vault_account.code_hash,
        CryptoHash::default(),
        "Vault contract was not deployed"
    );

    Ok(())
}
