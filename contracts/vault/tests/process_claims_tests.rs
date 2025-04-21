use anyhow::Ok;
use near_sdk::{json_types::U128, NearToken};
use near_workspaces::{network::Sandbox, sandbox, Account, Contract, Worker};
use serde_json::json;
use test_utils::{
    create_test_validator, initialize_test_token, initialize_test_vault_on_sub_account,
    make_accept_request_msg, register_account_with_token, UnstakeEntry, VaultViewState,
    VAULT_CALL_GAS,
};

#[path = "test_utils.rs"]
mod test_utils;

async fn setup_sandbox_and_accounts() -> anyhow::Result<(Worker<Sandbox>, Account, Account)> {
    // Setup worker and root
    let worker = sandbox().await?;
    let root = worker.root_account()?;

    // Get lender account
    let lender = root
        .create_subaccount("lender")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;

    Ok((worker, root, lender))
}

async fn setup_contracts(
    worker: &Worker<Sandbox>,
    root: &Account,
    lender: &Account,
) -> anyhow::Result<(Contract, Contract, Contract)> {
    // Deploy validator, token, and vault
    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault_on_sub_account(&root).await?.contract;

    // Register vault and lender with token
    for account in [vault.id(), lender.id()] {
        register_account_with_token(&root, &token, account).await?;
    }

    // Fund lender
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    Ok((validator, token, vault))
}

async fn request_and_accept_liquidity(
    root: &Account,
    lender: &Account,
    vault: &Contract,
    token: &Contract,
) -> anyhow::Result<()> {
    // Vault owner requests liquidity
    root.call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": token.id(),
            "amount": U128(1_000_000),
            "interest": U128(100_000),
            "collateral": NearToken::from_near(5),
            "duration": 86400
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fetch vault state to construct correct message
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let request = state
        .liquidity_request
        .expect("Expected liquidity_request to be present");

    // Lender sends ft_transfer_call to accept the request
    let msg = make_accept_request_msg(&request);
    let result = lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": request.amount,
            "msg": msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Verify event log was emitted
    let logs = result.logs();
    let matched = logs.iter().any(|log| {
        log.contains("EVENT_JSON") && log.contains(r#""event":"liquidity_request_accepted""#)
    });
    assert!(
        matched,
        "Expected liquidity_request_accepted event log not found: {:#?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn test_process_claims_fulfills_immediate_repayment() -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;

    // Setup contracts
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    // Delegate 5 NEAR from vault to validator
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(5)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward to simulate staking
    worker.fast_forward(1).await?;

    // Request and accept liquidity request
    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    // Patch accepted_at to simulate expiration
    vault
        .call("set_accepted_offer_timestamp")
        .args_json(json!({ "timestamp": 1_000_000_000 }))
        .transact()
        .await?
        .into_result()?;

    // Call process_claims after expiration
    let result = lender
        .call(vault.id(), "process_claims")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Check that liquidation was finalized
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_none(),
        "Expected liquidity_request to be cleared after full repayment"
    );
    assert!(
        state.accepted_offer.is_none(),
        "Expected accepted_offer to be cleared after full repayment"
    );

    // Verify liquidation_complete log was emitted
    let logs = result.logs();
    let matched = logs
        .iter()
        .any(|log| log.contains("EVENT_JSON") && log.contains(r#""event":"liquidation_complete""#));
    assert!(
        matched,
        "Expected liquidation_complete event not found: {:#?}",
        logs
    );

    Ok(())
}

#[tokio::test]
async fn test_process_claims_triggers_unstake_after_partial_repayment() -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;

    // Setup contracts
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    // Query the vault's available balance
    let available: U128 = vault.view("view_available_balance").await?.json()?;
    let available_yocto = available.0;

    // Compute how much to delegate (leave 2 NEAR for repayment)
    let leave_behind = NearToken::from_near(2).as_yoctonear();
    let to_delegate = available_yocto - leave_behind;
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(to_delegate)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward to simulate validator update
    worker.fast_forward(1).await?;

    // Request and accept liquidity request
    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    // Patch accepted_at to simulate expiration
    vault
        .call("set_accepted_offer_timestamp")
        .args_json(json!({ "timestamp": 1_000_000_000 }))
        .transact()
        .await?
        .into_result()?;

    // Call process_claims — should use 2 NEAR, unstake remaining 3 NEAR
    lender
        .call(vault.id(), "process_claims")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Check vault state — loan should still be active
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_some(),
        "Liquidity request should still be open"
    );
    assert!(
        state.accepted_offer.is_some(),
        "Accepted offer should still be active"
    );

    // Available balance should now be 0 (after partial repayment)
    let remaining: U128 = vault.view("view_available_balance").await?.json()?;
    assert_eq!(
        remaining.0, 0,
        "Expected available balance to be 0 after partial repayment"
    );

    // Check vault state to make sure there is an unstaked entry that is ~3NEAR
    let entry: UnstakeEntry = vault
        .view("get_unstake_entry")
        .args_json(json!({ "validator": validator.id() }))
        .await?
        .json()?;
    let rounded = entry.amount / 10u128.pow(24);
    assert_eq!(rounded, 3, "Expected 3 NEAR to be unstaked");

    Ok(())
}

#[tokio::test]
async fn test_process_claims_claims_matured_unstaked_near() -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;

    // Setup contracts
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    // Delegate most vault funds, leave 2 NEAR
    let available: U128 = vault.view("view_available_balance").await?.json()?;
    let leave_behind = NearToken::from_near(2).as_yoctonear();
    let to_delegate = available.0 - leave_behind;
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(to_delegate)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward to simulate validator update
    worker.fast_forward(1).await?;

    // Request and accept liquidity request
    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    // Patch accepted_at to simulate expiration
    vault
        .call("set_accepted_offer_timestamp")
        .args_json(json!({ "timestamp": 1_000_000_000 }))
        .transact()
        .await?
        .into_result()?;

    // Call process_claims — should use 2 NEAR, unstake remaining 3 NEAR
    root.call(vault.id(), "process_claims")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward 5 epochs to mature the unstake
    worker.fast_forward(5 * 500).await?;

    // Call process_claims again — should now trigger withdraw_all unstake_entries
    root.call(vault.id(), "process_claims")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Verify: loan is now fully repaid and state is cleared
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_none(),
        "Expected liquidity_request to be cleared after full repayment"
    );
    assert!(
        state.accepted_offer.is_none(),
        "Expected accepted_offer to be cleared after full repayment"
    );

    Ok(())
}

#[tokio::test]
async fn test_process_claims_waits_when_unstake_is_still_maturing() -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;

    // Setup contracts
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    // Leave ~2 NEAR, delegate the rest
    let available: U128 = vault.view("view_available_balance").await?.json()?;
    let to_delegate = available.0 - NearToken::from_near(2).as_yoctonear();
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(to_delegate)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward 1 epoch
    worker.fast_forward(1).await?;

    // Request and accept liquidity request
    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    // Patch accepted_at to simulate expiration
    vault
        .call("set_accepted_offer_timestamp")
        .args_json(json!({ "timestamp": 1_000_000_000 }))
        .transact()
        .await?
        .into_result()?;

    // Call process_claims — should use 2 NEAR, unstake remaining 3 NEAR
    lender
        .call(vault.id(), "process_claims")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward 1 block not enough for the 3 NEAR to be unstaked
    worker.fast_forward(1).await?;

    // Call process_claims again — should detect maturing, not claim
    let result = lender
        .call(vault.id(), "process_claims")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Expect: unstake_entry still exists
    let entry: Option<UnstakeEntry> = vault
        .view("get_unstake_entry")
        .args_json(json!({ "validator": validator.id() }))
        .await?
        .json()?;
    assert!(entry.is_some(), "Expected unstake_entry to still exist");

    // Expect: loan is still active
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_some(),
        "Expected loan to still be active"
    );
    assert!(
        state.accepted_offer.is_some(),
        "Expected accepted_offer to still be active"
    );

    // Expect: log indicates waiting
    let matched = result.logs().iter().any(|log| {
        log.contains("EVENT_JSON")
            && log.contains(r#""event":"liquidation_progress""#)
            && log.contains("waiting")
    });
    assert!(
        matched,
        "Expected liquidation_progress waiting log not found: {:#?}",
        result.logs()
    );

    Ok(())
}

#[tokio::test]
async fn test_process_claims_triggers_fallback_unstake_when_maturing_insufficient(
) -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;

    // Setup contracts
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    // Leave ~2 NEAR, delegate the rest
    let available: U128 = vault.view("view_available_balance").await?.json()?;
    let to_delegate = available.0 - NearToken::from_near(2).as_yoctonear();
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(to_delegate)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward 1 block
    worker.fast_forward(1).await?;

    // Undelegate 2 NEAR tokens
    root.call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(2)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward 1 block
    worker.fast_forward(1).await?;

    // Request and accept liquidity request
    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    // Patch accepted_at to simulate expiration
    vault
        .call("set_accepted_offer_timestamp")
        .args_json(json!({ "timestamp": 1_000_000_000 }))
        .transact()
        .await?
        .into_result()?;

    // Call process_claims — should
    // use 2 NEAR available,
    // see 2 NEAR tokens maturing
    // unbond the extra 1 NEAR to cover the deficit
    lender
        .call(vault.id(), "process_claims")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Expect the vault balance to be used
    let available: U128 = vault.view("view_available_balance").await?.json()?;
    assert_eq!(
        available.0, 0,
        "Expected vault balance to be 0 after partial repayment"
    );

    // Expect unstake entry for validator to now be ~3 NEAR
    let entry: UnstakeEntry = vault
        .view("get_unstake_entry")
        .args_json(json!({ "validator": validator.id() }))
        .await?
        .json()?;
    let unstaked_rounded = entry.amount / 10u128.pow(24);
    assert_eq!(
        unstaked_rounded, 3,
        "Expected ~3 NEAR to be in unstake_entries, got: {} yocto",
        entry.amount
    );

    Ok(())
}

#[tokio::test]
async fn test_process_claims_triggers_fallback_unstake_when_matured_insufficient(
) -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;

    // Setup contracts
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    // Leave ~2 NEAR, delegate the rest
    let available: U128 = vault.view("view_available_balance").await?.json()?;
    let to_delegate = available.0 - NearToken::from_near(2).as_yoctonear();
    root.call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_yoctonear(to_delegate)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward 1 block
    worker.fast_forward(1).await?;

    // Undelegate 2 NEAR tokens
    root.call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(2)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast-forward 1 block
    worker.fast_forward(1).await?;

    // Request and accept liquidity request
    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    // Patch accepted_at to simulate expiration
    vault
        .call("set_accepted_offer_timestamp")
        .args_json(json!({ "timestamp": 1_000_000_000 }))
        .transact()
        .await?
        .into_result()?;

    // Wait for more than 4 epochs for unstaked balance to mature
    worker.fast_forward(5 * 500).await?;

    // Call process_claims — should
    // use 2 NEAR available,
    // use 2 NEAR matured unstake entry
    // unbond the extra 1 NEAR to cover the deficit
    lender
        .call(vault.id(), "process_claims")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Expect the vault balance to be used
    let available: U128 = vault.view("view_available_balance").await?.json()?;
    assert_eq!(
        available.0, 0,
        "Expected vault balance to be 0 after partial repayment"
    );

    // Expect unstake entry for validator to now be ~1 NEAR
    let entry: UnstakeEntry = vault
        .view("get_unstake_entry")
        .args_json(json!({ "validator": validator.id() }))
        .await?
        .json()?;
    let unstaked_rounded = entry.amount / 10u128.pow(24);
    assert_eq!(
        unstaked_rounded, 1,
        "Expected ~1 NEAR to be in unstake_entries, got: {} yocto",
        entry.amount
    );

    Ok(())
}
