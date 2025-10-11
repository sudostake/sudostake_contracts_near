#![cfg(feature = "integration-test")]

use near_sdk::{json_types::U128, NearToken};
use near_workspaces::{network::Sandbox, Account, Worker};
use serde_json::{json, Value};
use test_utils::{
    create_test_validator, get_usdc_balance, initialize_test_token, initialize_test_vault,
    make_accept_request_msg, make_counter_offer_msg, register_account_with_token, VaultViewState,
    VAULT_CALL_GAS,
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
    let counter_lender: Account = root
        .create_subaccount("counter")
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
    register_account_with_token(&root, &token, counter_lender.id()).await?;

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
    root.call(token.id(), "ft_transfer")
        .args_json(serde_json::json!({
            "receiver_id": counter_lender.id(),
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

    // Counter lender submits a counter offer below the requested amount
    let counter_msg = make_counter_offer_msg(&request);
    counter_lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "900000",
            "msg": counter_msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

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

    // Counter offers should be cleared immediately after acceptance
    let offers: Option<serde_json::Map<String, Value>> =
        vault.view("get_counter_offers").await?.json()?;
    assert!(offers.is_none(), "Counter offers map was not cleared");

    Ok(())
}

#[tokio::test]
async fn test_accept_liquidity_request_refunds_on_token_mismatch() -> anyhow::Result<()> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let root: Account = worker.root_account()?;
    let lender: Account = root
        .create_subaccount("lender")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;

    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    register_account_with_token(&root, &token, vault.id()).await?;
    register_account_with_token(&root, &token, lender.id()).await?;

    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender.id(),
            "amount": "1000000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    // Provide NEAR so the vault can delegate for collateral validation
    root.transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;

    vault
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

    worker.fast_forward(1).await?;

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

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let request = state
        .liquidity_request
        .expect("Expected liquidity_request to exist");

    // Prepare a message with the wrong token while keeping all other fields valid
    let wrong_msg = serde_json::json!({
        "action": "AcceptLiquidityRequest",
        "token": "wrong-token.test.near",
        "amount": request.amount,
        "interest": request.interest,
        "collateral": request.collateral,
        "duration": request.duration
    })
    .to_string();

    let outcome = lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": request.amount,
            "msg": wrong_msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    assert!(
        outcome.is_success(),
        "Transfer call should succeed even when message is rejected"
    );

    // Lender balance should remain unchanged because the transfer is refunded
    let balance = get_usdc_balance(&token, lender.id()).await?;
    assert_eq!(
        balance.0, 1_000_000_000u128,
        "Lender balance should be fully refunded"
    );

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.accepted_offer.is_none(),
        "Offer should not be accepted"
    );
    assert!(
        state.liquidity_request.is_some(),
        "Liquidity request should remain open"
    );

    Ok(())
}

#[tokio::test]
async fn test_counter_offer_proposer_gets_refunded_on_accept() -> anyhow::Result<()> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let root: Account = worker.root_account()?;
    let lender: Account = root
        .create_subaccount("lender")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;

    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    register_account_with_token(&root, &token, vault.id()).await?;
    register_account_with_token(&root, &token, lender.id()).await?;

    // Fund lender with enough balance for both the counter offer and the final acceptance
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender.id(),
            "amount": "2000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    root.transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;

    vault
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

    worker.fast_forward(1).await?;

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

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let request = state
        .liquidity_request
        .expect("Expected liquidity_request to exist");

    // Submit a counter offer below the requested amount
    let counter_msg = make_counter_offer_msg(&request);
    lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "900000",
            "msg": counter_msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Accept the request with the same lender
    let accept_msg = make_accept_request_msg(&request);
    let outcome = lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": request.amount,
            "msg": accept_msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    assert!(outcome.is_success(), "Acceptance call should succeed");

    // Counter offers should be fully cleared from state
    let offers: Option<serde_json::Map<String, Value>> =
        vault.view("get_counter_offers").await?.json()?;
    assert!(
        offers.is_none(),
        "Counter offers map should be cleared after acceptance"
    );

    // The lender should retain only the request amount (their counter offer refunded)
    let balance = get_usdc_balance(&token, lender.id()).await?;
    assert_eq!(
        balance.0, 1_000_000u128,
        "Lender should be refunded their counter offer amount"
    );

    // Owner attempts to open a new request immediately — expect rejection because the
    // current loan is accepted, but the error must not mention lingering counter offers.
    let duplicate_request = root
        .call(vault.id(), "request_liquidity")
        .args_json(json!({
            "token": token.id(),
            "amount": U128(500_000),
            "interest": U128(50_000),
            "collateral": NearToken::from_near(5),
            "duration": 43200
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    assert!(duplicate_request.is_failure());
    let err = duplicate_request.into_result().unwrap_err().to_string();
    assert!(
        err.contains("A request is already open") || err.contains("Vault is already matched"),
        "Unexpected error message: {err}"
    );
    assert!(
        !err.contains("Counter-offers must be cleared"),
        "Counter offer guard should no longer block new requests"
    );

    Ok(())
}
