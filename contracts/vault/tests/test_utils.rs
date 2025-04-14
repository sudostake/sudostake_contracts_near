#![allow(dead_code)]

use anyhow::Ok;
use near_primitives::types::AccountId;
use near_sdk::json_types::U128;
use near_sdk::{Gas, NearToken};
use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::types::SecretKey;
use near_workspaces::{network::Sandbox, Account, Contract, Worker};
use serde_json::json;

const VAULT_WASM_PATH: &str = "../../res/vault.wasm";
const STAKING_POOL_WASM_PATH: &str = "../../res/mock_staking_pool.wasm";
const FT_WASM_PATH: &str = "../../res/fungible_token.wasm";
const FT_TOTAL_SUPPLY: &str = "1000000000000"; // 1,000,000 USDC (1_000_000 × 10^6)
const FT_DECIMALS: u8 = 6;
pub const VAULT_CALL_GAS: Gas = Gas::from_tgas(300);

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

#[derive(serde::Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct LiquidityRequest {
    pub token: AccountId,
    pub amount: U128,
    pub interest: U128,
    pub collateral: NearToken,
    pub duration: u64,
    pub created_at: u64,
}

#[derive(serde::Deserialize, Debug)]
#[serde(crate = "near_sdk::serde")]
pub struct VaultViewState {
    pub owner: String,
    pub index: u64,
    pub version: u64,
    pub pending_liquidity_request: Option<serde_json::Value>,
    pub liquidity_request: Option<LiquidityRequest>,
    pub accepted_offer: Option<serde_json::Value>,
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

// TODO consolidate this method with initialize_test_vault
pub async fn initialize_test_vault_on_sub_account(
    root: &Account,
) -> anyhow::Result<InstantiateTestVaultResult> {
    // Read the vault wasm file
    let vault_wasm = std::fs::read(VAULT_WASM_PATH)?;

    // Create a new subaccount for the vault (unique name)
    let subaccount = root
        .create_subaccount("vault")
        .initial_balance(NearToken::from_near(5))
        .transact()
        .await?
        .into_result()?;

    // Deploy the vault to that subaccount
    let vault: Contract = subaccount.deploy(&vault_wasm).await?.into_result()?;

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

pub async fn initialize_test_token(root: &Account) -> anyhow::Result<Contract> {
    // Read the token wasm file
    let ft_wasm = std::fs::read(FT_WASM_PATH)?;

    // Create a new subaccount for the token (unique name)
    let subaccount = root
        .create_subaccount("token")
        .initial_balance(NearToken::from_near(5))
        .transact()
        .await?
        .into_result()?;

    // Deploy the token to that subaccount
    let token: Contract = subaccount.deploy(&ft_wasm).await?.into_result()?;

    // Call `new` with proper USDC-style metadata (6 decimals)
    token
        .call("new")
        .args_json(json!({
            "owner_id": root.id(),
            "total_supply": FT_TOTAL_SUPPLY,
            "metadata": {
                "spec": "ft-1.0.0",
                "name": "Mock USD Coin",
                "symbol": "USDC",
                "decimals": FT_DECIMALS
            }
        }))
        .transact()
        .await?
        .into_result()?;

    Ok(token)
}

pub async fn register_account_with_token(
    root: &Account,
    token: &Contract,
    account_id: &AccountId,
) -> anyhow::Result<()> {
    root.call(token.id(), "storage_deposit")
        .args_json(json!({ "account_id": account_id }))
        .deposit(NearToken::from_yoctonear(125_000_000_000_000_000_000_000)) // ≈ 0.00125 NEAR
        .transact()
        .await?
        .into_result()?;

    Ok(())
}

pub async fn transfer_tokens_to_vault(
    root: &Account,
    token: &Contract,
    vault: &Contract,
    amount: u128,
    msg: &str,
) -> anyhow::Result<ExecutionFinalResult> {
    let result = root
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": amount.to_string(),
            "msg": msg,
        }))
        .deposit(NearToken::from_yoctonear(1)) // required by NEP-141
        .max_gas()
        .transact()
        .await?;

    Ok(result)
}

pub async fn withdraw_ft(
    vault: &Contract,
    token: &Contract,
    caller: &Account,
    recipient: &Account,
    amount: u128,
) -> anyhow::Result<ExecutionFinalResult> {
    let result = caller
        .call(vault.id(), "withdraw_balance")
        .args_json(json!({
            "token_address": token.id(),
            "amount": amount.to_string(),
            "to": recipient.id()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    Ok(result)
}

pub fn make_accept_request_msg(request: &LiquidityRequest) -> String {
    serde_json::json!({
        "action": "AcceptLiquidityRequest",
        "token": request.token,
        "amount": request.amount,
        "interest": request.interest,
        "collateral": request.collateral,
        "duration": request.duration
    })
    .to_string()
}
