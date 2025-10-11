use anyhow::Ok;
use near_sdk::{env, Gas, NearToken};
use near_workspaces::types::CryptoHash;
use near_workspaces::{self as workspaces, network::Sandbox, Account, Contract, Worker};
use serde_json::{json, Value};

const FACTORY_WASM_PATH: &str = "../../res/factory.wasm";
const VAULT_WASM_PATH: &str = "../../res/vault.wasm";

// NEAR costs (yoctoNEAR)
const STORAGE_BUFFER: u128 = 10_000_000_000_000_000_000_000; // 0.01 NEAR
pub const FACTORY_CALL_GAS: Gas = Gas::from_tgas(300);

async fn calculate_minting_fee() -> anyhow::Result<NearToken> {
    let vault_wasm = std::fs::read(VAULT_WASM_PATH)?;
    let wasm_bytes = vault_wasm.len() as u128;
    let deploy_cost = wasm_bytes * env::storage_byte_cost().as_yoctonear();
    let total_fee_yocto = deploy_cost + STORAGE_BUFFER;

    Ok(NearToken::from_yoctonear(total_fee_yocto))
}

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

    // Calculate minting fee
    let minting_fee = calculate_minting_fee().await?;

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
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?;

    // Check if the call was a success
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

#[tokio::test]
async fn test_withdraw_balance_after_vault_mint() -> anyhow::Result<()> {
    // Set up sandbox and create independent accounts
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let user = worker.dev_create_account().await?;
    let factory = worker.dev_create_account().await?;

    // Deploy factory contract
    let factory_contract = factory
        .deploy(&std::fs::read(FACTORY_WASM_PATH)?)
        .await?
        .into_result()?;

    // Calculate minting fee
    let minting_fee = calculate_minting_fee().await?;

    // Initialize factory with calculated minting fee
    owner
        .call(factory_contract.id(), "new")
        .args_json(serde_json::json!({
            "owner": owner.id(),
            "vault_minting_fee": minting_fee
        }))
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Call mint_vault with the correct fee
    user.call(factory.id(), "mint_vault")
        .deposit(minting_fee.clone())
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Record balance before withdrawal
    let balance_before = owner.view_account().await?.balance;

    // Withdraw 1 NEAR back to the owner
    owner
        .call(factory_contract.id(), "withdraw_balance")
        .args_json(serde_json::json!({
            "amount": NearToken::from_yoctonear(1_000_000_000_000_000_000_000_000),
            "to_address": null
        }))
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Record balance after withdrawal
    let balance_after = owner.view_account().await?.balance;

    // Confirm owner's balance increased
    assert!(
        balance_after > balance_before,
        "Owner should have received withdrawal after vault mint"
    );

    Ok(())
}

#[tokio::test]
async fn test_withdraw_balance_success_to_third_party() -> anyhow::Result<()> {
    // Create sandbox and accounts
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let user = worker.dev_create_account().await?;
    let receiver = worker.dev_create_account().await?;
    let factory = worker.dev_create_account().await?;

    // Deploy factory contract
    let factory_contract = factory
        .deploy(&std::fs::read(FACTORY_WASM_PATH)?)
        .await?
        .into_result()?;

    // Calculate minting fee
    let minting_fee = calculate_minting_fee().await?;

    // Initialize factory with calculated minting fee
    owner
        .call(factory_contract.id(), "new")
        .args_json(serde_json::json!({
            "owner": owner.id(),
            "vault_minting_fee": minting_fee
        }))
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Call mint_vault with the correct fee
    user.call(factory.id(), "mint_vault")
        .deposit(minting_fee.clone())
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Record balance before
    let balance_before = receiver.view_account().await?.balance;

    // Owner withdraws 1 NEAR to third-party (receiver)
    owner
        .call(factory_contract.id(), "withdraw_balance")
        .args_json(serde_json::json!({
            "amount": NearToken::from_yoctonear(1_000_000_000_000_000_000_000_000),
            "to_address": receiver.id(),
        }))
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let balance_after = receiver.view_account().await?.balance;

    // Confirm receiver's balance increased
    assert!(
        balance_after > balance_before,
        "Receiver should have received third-party withdrawal"
    );

    Ok(())
}

#[tokio::test]
async fn test_withdraw_balance_success_full_available_balance() -> anyhow::Result<()> {
    // Set up sandbox environment and accounts
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let user = worker.dev_create_account().await?;
    let factory_account = worker.dev_create_account().await?;

    // Deploy and initialize the factory contract
    let factory_contract = factory_account
        .deploy(&std::fs::read(FACTORY_WASM_PATH)?)
        .await?
        .into_result()?;

    // Calculate minting fee
    let minting_fee = calculate_minting_fee().await?;

    // Initialize factory with calculated minting fee
    owner
        .call(factory_contract.id(), "new")
        .args_json(serde_json::json!({
            "owner": owner.id(),
            "vault_minting_fee": minting_fee
        }))
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Call mint_vault with the correct fee
    user.call(factory_contract.id(), "mint_vault")
        .deposit(minting_fee.clone())
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Query factory account state before withdrawal
    let factory_account_info = worker.view_account(factory_contract.id()).await?;
    let balance_before = factory_account_info.balance.as_yoctonear();
    let storage_usage = factory_account_info.storage_usage;

    // Fetch dynamic storage byte cost from contract
    let storage_byte_cost: NearToken = factory_contract
        .view("storage_byte_cost")
        .args_json(serde_json::json!({}))
        .await?
        .json()?;

    let storage_cost = storage_usage as u128 * storage_byte_cost.as_yoctonear();
    let withdraw_amount = NearToken::from_yoctonear(balance_before - storage_cost);

    // Capture owner balance before withdrawal
    let owner_balance_before = owner.view_account().await?.balance;

    // Perform withdrawal to owner account
    owner
        .call(factory_contract.id(), "withdraw_balance")
        .args_json(serde_json::json!({
            "amount": withdraw_amount,
            "to_address": null
        }))
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Validate post-withdrawal balances
    let factory_balance_after = worker.view_account(factory_contract.id()).await?.balance;
    let owner_balance_after = owner.view_account().await?.balance;

    assert!(
        factory_balance_after.as_yoctonear() >= storage_cost,
        "Factory should retain at least the required storage cost"
    );

    assert!(
        owner_balance_after > owner_balance_before,
        "Owner balance should increase after withdrawal"
    );

    Ok(())
}

#[tokio::test]
async fn test_transfer_ownership_updates_owner_in_state() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let new_owner = worker.dev_create_account().await?;
    let factory_account = worker.dev_create_account().await?;

    let factory_contract = factory_account
        .deploy(&std::fs::read(FACTORY_WASM_PATH)?)
        .await?
        .into_result()?;

    let minting_fee = calculate_minting_fee().await?;

    owner
        .call(factory_contract.id(), "new")
        .args_json(serde_json::json!({
            "owner": owner.id(),
            "vault_minting_fee": minting_fee
        }))
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    owner
        .call(factory_contract.id(), "transfer_ownership")
        .args_json(serde_json::json!({
            "new_owner": new_owner.id(),
        }))
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let state: serde_json::Value = factory_contract
        .view("get_contract_state")
        .args_json(serde_json::json!({}))
        .await?
        .json()?;

    assert_eq!(state["owner"], Value::String(new_owner.id().to_string()));

    Ok(())
}

#[tokio::test]
async fn test_get_contract_state_reflects_latest_values() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let owner = worker.dev_create_account().await?;
    let user = worker.dev_create_account().await?;
    let factory_account = worker.dev_create_account().await?;

    let factory_contract = factory_account
        .deploy(&std::fs::read(FACTORY_WASM_PATH)?)
        .await?
        .into_result()?;

    let minting_fee = calculate_minting_fee().await?;

    owner
        .call(factory_contract.id(), "new")
        .args_json(serde_json::json!({
            "owner": owner.id(),
            "vault_minting_fee": minting_fee.clone()
        }))
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    user
        .call(factory_contract.id(), "mint_vault")
        .deposit(minting_fee.clone())
        .gas(FACTORY_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let state: serde_json::Value = factory_contract
        .view("get_contract_state")
        .args_json(serde_json::json!({}))
        .await?
        .json()?;

    assert_eq!(state["owner"], Value::String(owner.id().to_string()));
    assert_eq!(state["vault_counter"].as_u64(), Some(1));
    let expected_fee = minting_fee.as_yoctonear().to_string();
    assert_eq!(
        state["vault_minting_fee"].as_str(),
        Some(expected_fee.as_str())
    );

    Ok(())
}
