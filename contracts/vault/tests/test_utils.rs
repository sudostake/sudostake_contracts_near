#![allow(dead_code)]

use anyhow::Ok;
use near_primitives::types::AccountId;
use near_sdk::{Gas, NearToken};
use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::types::SecretKey;
use near_workspaces::{network::Sandbox, Account, Contract, Worker};
use serde_json::json;

const VAULT_WASM_PATH: &str = "../../res/vault.wasm";
const STAKING_POOL_WASM_PATH: &str = "../../res/mock_staking_pool.wasm";

pub struct InstantiateTestVaultResult {
    pub execution_result: ExecutionFinalResult,
    pub contract: Contract,
}

#[derive(serde::Deserialize, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct UnstakeEntry {
    pub amount: u128,
    pub epoch_height: u64,
}

pub async fn create_test_validator(
    worker: &Worker<Sandbox>,
    root: &Account,
) -> anyhow::Result<Contract> {
    // Deploy staking_pool.wasm to validator.poolv1.near
    let staking_pool_wasm = std::fs::read(STAKING_POOL_WASM_PATH)?;
    let validator: Contract = root
        .create_subaccount("validator")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?
        .deploy(&staking_pool_wasm)
        .await?
        .into_result()?;

    // Generate a keypair
    let account_id: AccountId = "validator".parse()?;
    let secret_key = SecretKey::from_random(near_workspaces::types::KeyType::ED25519);
    let public_key = secret_key.public_key();
    let validator_pk_str = public_key.to_string();

    // Create TLA with the key
    let validator_owner = worker
        .create_tla(account_id.clone(), secret_key.clone())
        .await?
        .into_result()?;

    // Instantiate a new validator instance
    let _ = validator
        .call("new")
        .args_json(json!({
            "owner_id": validator_owner.id(),
            "stake_public_key": validator_pk_str,
            "reward_fee_fraction": {
                "numerator": 0,
                "denominator": 100
            }
        }))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Return the newly created validator contract
    Ok(validator)
}

pub async fn initialize_test_vault(root: &Account) -> anyhow::Result<InstantiateTestVaultResult> {
    // Deploy the vault contract
    let vault_wasm = std::fs::read(VAULT_WASM_PATH)?;
    let vault: Contract = root.deploy(&vault_wasm).await?.into_result()?;

    // Initialize the vault contract
    let res = vault
        .call("new")
        .args_json(json!({
            "owner": root.id(),
            "index": 0,
            "version": 1
        }))
        .transact()
        .await?;

    Ok(InstantiateTestVaultResult {
        execution_result: res,
        contract: vault,
    })
}
