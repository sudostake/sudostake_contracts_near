use near_sdk::{json_types::U128, NearToken};
use near_workspaces::{network::Sandbox, Account, Worker};
use serde_json::json;
use test_utils::{
    create_test_validator, initialize_test_token, initialize_test_vault, make_accept_request_msg,
    register_account_with_token, VaultViewState, VAULT_CALL_GAS,
};

#[path = "test_utils.rs"]
mod test_utils;

#[tokio::test]
async fn test_accept_liquidity_request_succeeds() -> anyhow::Result<()> {
    // Set up sandbox and accounts
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let root: Account = worker.root_account()?;
    let lender: Account = root
        .create_subaccount("lender")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;

    // Initialize a new validator
    let validator = create_test_validator(&worker, &root).await?;

    // Deploy token and vault
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    // Register vault and lender with token
    register_account_with_token(&root, &token, vault.id()).await?;
    register_account_with_token(&root, &token, lender.id()).await?;

    // Mint some tokens to lender from the token owner
    root.call(token.id(), "ft_transfer")
        .args_json(serde_json::json!({
            "receiver_id": lender.id(),
            "amount": "1000000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    // Fund the vault with NEAR so it can delegate
    root.transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;

    // Delegate 5 NEAR to the validator
    let _ = vault
        .call("delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(5),
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fast forward 1 block so stake is visible to staking pool
    worker.fast_forward(1).await?;

    // Open a liquidity request
    vault
        .call("request_liquidity")
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
