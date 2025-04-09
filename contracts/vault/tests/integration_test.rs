use anyhow::Ok;
use near_primitives::types::AccountId;
use near_sdk::{Gas, NearToken};
use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::types::SecretKey;
use near_workspaces::{network::Sandbox, Account, Contract, Worker};
use serde_json::json;

const VAULT_WASM_PATH: &str = "../../res/vault.wasm";
const STAKING_POOL_WASM_PATH: &str = "../../res/staking_pool.wasm";

struct InstantiateTestVaultResult {
    pub execution_result: ExecutionFinalResult,
    pub contract: Contract,
}

async fn create_test_validator(
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

async fn initialize_test_vault(root: &Account) -> anyhow::Result<InstantiateTestVaultResult> {
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

#[tokio::test]
async fn test_delegate_fast_path() -> anyhow::Result<()> {
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
    let _ = root
        .transfer_near(vault.id(), NearToken::from_near(10))
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

    // Verify that delegate_direct was called
    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains("delegate_direct")),
        "Expected 'delegate_direct' log event to be emitted"
    );

    // Verify that validator_activated was called
    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains("validator_activated")),
        "Expected 'validator_activated' log event to be emitted"
    );

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

#[tokio::test]
async fn test_delegate_with_reconciliation_happy_path() -> anyhow::Result<()> {
    // Set up sandbox
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault: Contract = res.contract;

    // Transfer 5 NEAR from root to vault
    let _ = root
        .transfer_near(vault.id(), NearToken::from_near(5))
        .await?
        .into_result()?;

    // Call `delegate` with 2 NEAR and attach 1 yoctoNEAR for assert_one_yocto
    let _ = vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(2)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result();

    // Initially undelegate 1 NEAR to create unstake entry
    let _ = vault
        .call("undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Wait for unbonding window to pass
    worker.fast_forward(5).await?;

    // Now delegate again — should trigger reconciliation
    let result = vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Extract logs
    let logs = result.logs();

    // Check full path was used (no delegate_direct)
    assert!(
        !logs.iter().any(|log| log.contains("delegate_direct")),
        "Expected full path, but found 'delegate_direct' log"
    );

    // Reconciliation log should appear
    assert!(
        logs.iter()
            .any(|log| log.contains("unstake_entries_reconciled")),
        "Expected 'unstake_entries_reconciled' log not found"
    );

    // Final staking log should confirm
    assert!(
        logs.iter().any(|log| log.contains("delegate_completed")),
        "Expected 'delegate_completed' log not found"
    );

    Ok(())
}

#[tokio::test]
async fn test_undelegate_happy_path() -> anyhow::Result<()> {
    // Initialize sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault: Contract = res.contract;

    // Fund the vault with 5 NEAR from the root
    let _ = root
        .transfer_near(vault.id(), NearToken::from_near(5))
        .await?
        .into_result()?;

    // Call delegate for 3 NEAR to the validator
    vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(3)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Call undelegate for 1 NEAR from the validator
    let result = vault
        .call("undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Check for the expected unstake entry log
    let logs = result.logs();
    let found = logs.iter().any(|log| log.contains("unstake_entry_added"));
    assert!(
        found,
        "Expected 'unstake_entry_added' log not found. Logs: {:?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn test_undelegate_with_reconciliation_happy_path() -> anyhow::Result<()> {
    // Initialize sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Instantiate the vault contract
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault: Contract = res.contract;

    // Fund the vault with 5 NEAR from the root
    let _ = root
        .transfer_near(vault.id(), NearToken::from_near(5))
        .await?
        .into_result()?;

    // Call delegate for 2 NEAR to the validator
    vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(2)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Call undelegate for 1 NEAR from the validator
    let result = vault
        .call("undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Check for the expected unstake entry log
    let logs = result.logs();
    let found = logs.iter().any(|log| log.contains("unstake_entry_added"));
    assert!(
        found,
        "Expected 'unstake_entry_added' log not found. Logs: {:?}",
        logs
    );

    // Wait 5 epochs to pass unbonding period
    worker.fast_forward(5).await?;

    // Call undelegate again to trigger reconciliation before new unstake
    let result = vault
        .call("undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Extract logs
    let logs = result.logs();

    // Confirm reconciliation was triggered
    let found_reconcile = logs
        .iter()
        .any(|log| log.contains("unstake_entries_reconciled"));
    assert!(
        found_reconcile,
        "Expected 'unstake_entries_reconciled' log not found. Logs: {:?}",
        logs
    );

    // Confirm a second unstake entry was added
    let found_new_unstake = logs
        .iter()
        .filter(|log| log.contains("unstake_entry_added"))
        .count();
    assert_eq!(
        found_new_unstake, 1,
        "Expected exactly one new 'unstake_entry_added' log. Logs: {:?}",
        logs
    );

    // Confirm validator was removed
    assert!(
        logs.iter().any(|log| log.contains("validator_removed")),
        "Expected 'validator_removed' log not found. Logs: {:?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_unstaked_happy_path() -> anyhow::Result<()> {
    // Set up the sandbox environment and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Deploy and initialize the validator (staking pool)
    let validator = create_test_validator(&worker, &root).await?;

    // Deploy and initialize the vault contract
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault = res.contract;

    // Fund the vault with 5 NEAR
    root.transfer_near(vault.id(), NearToken::from_near(5))
        .await?
        .into_result()?;

    // Delegate 2 NEAR to validator
    vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(2)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // undelegate 1 NEAR from validator
    vault
        .call("undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Wait 5 epochs to allow unbonding to complete
    worker.fast_forward(5).await?;

    // Call claim_unstaked to trigger withdraw_all + reconciliation
    let result = vault
        .call("claim_unstaked")
        .args_json(json!({ "validator": validator.id() }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Extract logs
    let logs = result.logs();
    let found_reconciled = logs
        .iter()
        .any(|log| log.contains("unstake_entries_reconciled"));
    let found_completed = logs
        .iter()
        .any(|log| log.contains("claim_unstaked_completed"));

    // Confirm unstake_entries_reconciled
    assert!(
        found_reconciled,
        "Expected 'unstake_entries_reconciled' log not found. Logs: {:#?}",
        logs
    );

    // Confirm claim_unstaked_completed
    assert!(
        found_completed,
        "Expected 'claim_unstaked_completed' log not found. Logs: {:#?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_unstaked_partial_withdraw() -> anyhow::Result<()> {
    // Set up the sandbox and root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Deploy and initialize the validator
    let validator = create_test_validator(&worker, &root).await?;

    // Deploy and initialize the vault contract
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault = res.contract;

    // Fund the vault with 5 NEAR
    root.transfer_near(vault.id(), NearToken::from_near(5))
        .await?
        .into_result()?;

    // Delegate 2 NEAR
    vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(2)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Undelegate 0.4 NEAR (creates the first unstake entry)
    vault
        .call("undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(400_000_000_000_000_000_000_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Wait 3 epochs so the first unstake entry is almost eligible for withdrawal
    worker.fast_forward(3).await?;

    // Undelegate another 0.6 NEAR (creates a second entry still within bonding window)
    vault
        .call("undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(600_000_000_000_000_000_000_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?;

    // Wait 2 more epochs — total of 5 epochs since first unstake, but only 2 for the second
    // This means only the first (0.4 NEAR) will be eligible for withdrawal
    worker.fast_forward(2).await?;

    // Call `claim_unstaked` — this will call withdraw_all + reconciliation logic
    let result = vault
        .call("claim_unstaked")
        .args_json(json!({ "validator": validator.id() }))
        .deposit(NearToken::from_yoctonear(1)) // assert_one_yocto required
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Read the logs emitted during the call
    let logs = result.logs();

    // Check that reconciliation occurred (should remove first entry)
    let found_reconciled = logs
        .iter()
        .any(|log| log.contains("unstake_entries_reconciled"));

    // Check that the claim_unstaked flow was completed
    let found_completed = logs
        .iter()
        .any(|log| log.contains("claim_unstaked_completed"));

    assert!(
        found_reconciled,
        "Expected 'unstake_entries_reconciled' log not found. Logs: {:#?}",
        logs
    );

    assert!(
        found_completed,
        "Expected 'claim_unstaked_completed' log not found. Logs: {:#?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_unstaked_works_gracefully_when_queue_is_empty() -> anyhow::Result<()> {
    // Start sandbox and get root account
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    // Deploy a fresh validator contract
    let validator = create_test_validator(&worker, &root).await?;

    // Deploy and initialize a new Vault contract instance
    let res = initialize_test_vault(&root).await?;
    res.execution_result.into_result()?;
    let vault = res.contract;

    // Fund the vault with NEAR so it's able to pay for gas and storage
    root.transfer_near(vault.id(), NearToken::from_near(2))
        .await?
        .into_result()?;

    // Immediately call claim_unstaked without ever undelegating
    let result = vault
        .call("claim_unstaked")
        .args_json(json!({
            "validator": validator.id()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(Gas::from_tgas(300))
        .transact()
        .await?;

    // Check logs to confirm reconciliation and completion occurred
    let logs = result.logs();

    let found_reconciled = logs
        .iter()
        .any(|log| log.contains("unstake_entries_reconciled"));
    let found_completed = logs
        .iter()
        .any(|log| log.contains("claim_unstaked_completed"));

    assert!(
        found_reconciled,
        "Expected 'unstake_entries_reconciled' log not found. Logs: {:#?}",
        logs
    );

    assert!(
        found_completed,
        "Expected 'claim_unstaked_completed' log not found. Logs: {:#?}",
        logs
    );

    Ok(())
}
