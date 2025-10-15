#![cfg(feature = "integration-test")]

use anyhow::Ok;
use near_sdk::{json_types::U128, NearToken};
use serde_json::json;
use test_utils::{
    create_test_validator, get_usdc_balance, initialize_test_token, initialize_test_vault,
    make_apply_counter_offer_msg, register_account_with_token, VaultViewState, VAULT_CALL_GAS,
};

#[path = "test_utils.rs"]
mod test_utils;

#[tokio::test]
async fn test_repay_loan_flow_successfully_clears_loan() -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let lender = root
        .create_subaccount("lender")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;

    // Deploy validator, token, and vault
    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    // Register vault and lender with token
    for account in [vault.id(), lender.id()] {
        register_account_with_token(&root, &token, account).await?;
    }

    // Fund lender with 1_000_000 USDC
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

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

    // Fetch request details for offer message
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let request = state
        .liquidity_request
        .expect("Liquidity request not found");

    // Lender sends a counter offer
    let msg = make_apply_counter_offer_msg(&request);
    lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "900000",
            "msg": msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Vault owner accepts lender’s offer
    root.call(vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": lender.id(),
            "amount": U128(900_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Vault owner repays the loan (principal + interest)
    root.call(vault.id(), "repay_loan")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Check vault state is cleared
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.accepted_offer.is_none(),
        "accepted_offer should be None after repayment"
    );
    assert!(
        state.liquidity_request.is_none(),
        "liquidity_request should be None after repayment"
    );

    // Lender's balance should now be 1,100,000
    // Originally had 1,000,000 funded
    // proposed a counter offer with 900,000
    // Got back 1,000,000 as repayment from the loan (principal + interest)
    // Total now 1,100,000
    let balance = get_usdc_balance(&token, lender.id()).await?;
    assert_eq!(
        balance.0, 1_100_000,
        "Lender should receive repayment of 1.0M on top of original 100K left"
    );

    Ok(())
}

#[tokio::test]
async fn test_repay_loan_releases_rights_to_undelegate() -> anyhow::Result<()> {
    // Setup sandbox and accounts
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let lender = root
        .create_subaccount("lender")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;

    // Deploy validator, token, and vault
    let validator = create_test_validator(&worker, &root).await?;
    let token = initialize_test_token(&root).await?;
    let vault = initialize_test_vault(&root).await?.contract;

    // Register vault and lender with token
    for account in [vault.id(), lender.id()] {
        register_account_with_token(&root, &token, account).await?;
    }

    // Fund lender with 1_000_000 USDC
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender.id(),
            "amount": "1000000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

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

    // Lender submits a counter offer
    let request: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let offer_msg = make_apply_counter_offer_msg(&request.liquidity_request.unwrap());
    lender
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "900000",
            "msg": offer_msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Vault owner accepts the offer
    root.call(vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": lender.id(),
            "amount": U128(900_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Vault owner attempts to undelegate now — should fail with expected message
    let result = root
        .call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Cannot undelegate when a liquidity request is open"),
        "Expected undelegate to fail after accepting offer, got: {failure_text}"
    );

    // Repay the loan
    root.call(vault.id(), "repay_loan")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Try undelegating again — should now succeed
    let tx = root
        .call(vault.id(), "undelegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;
    assert!(
        tx.is_success(),
        "Expected undelegate to succeed after repayment"
    );

    Ok(())
}
