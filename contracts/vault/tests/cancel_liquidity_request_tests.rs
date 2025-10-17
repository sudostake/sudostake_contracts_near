#![cfg(feature = "integration-test")]

use near_sdk::{json_types::U128, NearToken};
use near_workspaces::{network::Sandbox, Account, Contract, Worker};
use serde_json::json;
use test_utils::{
    create_test_validator, get_usdc_balance, initialize_test_token,
    initialize_test_vault_on_sub_account, make_apply_counter_offer_msg,
    register_account_with_token, InstantiateTestVaultResult, RefundEntry, VaultViewState,
    VAULT_CALL_GAS,
};

#[path = "test_lock.rs"]
mod test_lock;
#[path = "test_utils.rs"]
mod test_utils;

struct CancelLiquidityTestEnv {
    worker: Worker<Sandbox>,
    root: Account,
    vault: Contract,
    token: Contract,
}

async fn setup_cancel_liquidity_env() -> anyhow::Result<CancelLiquidityTestEnv> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    let InstantiateTestVaultResult {
        contract: vault, ..
    } = initialize_test_vault_on_sub_account(&root).await?;

    let token = initialize_test_token(&root).await?;
    let validator = create_test_validator(&worker, &root).await?;

    root.transfer_near(vault.id(), NearToken::from_near(10))
        .await?
        .into_result()?;

    root.call(vault.id(), "delegate")
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

    Ok(CancelLiquidityTestEnv {
        worker,
        root,
        vault,
        token,
    })
}

#[tokio::test]
async fn cancel_liquidity_request_clears_state() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let CancelLiquidityTestEnv {
        worker: _worker,
        root,
        vault,
        token: _,
    } = setup_cancel_liquidity_env().await?;

    root.call(vault.id(), "cancel_liquidity_request")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_none(),
        "Expected liquidity_request to be None"
    );

    Ok(())
}

#[tokio::test]
async fn cancel_liquidity_request_requires_exact_yocto() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let CancelLiquidityTestEnv {
        worker: _worker,
        root,
        vault,
        ..
    } = setup_cancel_liquidity_env().await?;

    let result = root
        .call(vault.id(), "cancel_liquidity_request")
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Requires attached deposit of exactly 1 yoctoNEAR"),
        "Expected panic due to missing 1 yoctoNEAR. Got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn cancel_liquidity_request_rejects_non_owner() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let CancelLiquidityTestEnv { worker, vault, .. } = setup_cancel_liquidity_env().await?;

    let alice = worker.dev_create_account().await?;

    let result = alice
        .call(vault.id(), "cancel_liquidity_request")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Only the vault owner can cancel the liquidity request"),
        "Expected panic for non-owner cancellation. Got: {failure_text}"
    );

    drop(worker);
    Ok(())
}

#[tokio::test]
async fn cancel_liquidity_request_refunds_all_counter_offers() -> anyhow::Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let CancelLiquidityTestEnv {
        worker: _worker,
        root,
        vault,
        token,
    } = setup_cancel_liquidity_env().await?;

    let lender_a = root
        .create_subaccount("lender-a")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;
    let lender_b = root
        .create_subaccount("lender-b")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?;

    register_account_with_token(&root, &token, vault.id()).await?;
    register_account_with_token(&root, &token, lender_a.id()).await?;
    register_account_with_token(&root, &token, lender_b.id()).await?;

    // Prefund lenders with USDC
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender_a.id(),
            "amount": "900000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;
    root.call(token.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": lender_b.id(),
            "amount": "950000"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let initial_balance_a = get_usdc_balance(&token, lender_a.id()).await?;
    let initial_balance_b = get_usdc_balance(&token, lender_b.id()).await?;

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let request = state
        .liquidity_request
        .as_ref()
        .expect("request should be open");
    let msg = make_apply_counter_offer_msg(request);

    lender_a
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

    lender_b
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "950000",
            "msg": msg
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let balance_a_before_refund = get_usdc_balance(&token, lender_a.id()).await?;
    let balance_b_before_refund = get_usdc_balance(&token, lender_b.id()).await?;
    assert_eq!(
        balance_a_before_refund.0, 0,
        "Lender A should have transferred full balance to the vault"
    );
    assert_eq!(
        balance_b_before_refund.0, 0,
        "Lender B should have transferred full balance to the vault"
    );

    let result = root
        .call(vault.id(), "cancel_liquidity_request")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;
    result.into_result()?;

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_none(),
        "Liquidity request should be cleared"
    );

    let offers: serde_json::Value = vault.view("get_counter_offers").await?.json()?;
    assert!(
        offers.is_null(),
        "Counter offers map should be cleared after cancellation"
    );

    let refunds: Vec<(u64, RefundEntry)> = vault
        .view("get_refund_entries")
        .args_json(json!({}))
        .await?
        .json()?;
    assert!(
        refunds.is_empty(),
        "Refund list should be empty after successful refunds"
    );

    let balance_a_after = get_usdc_balance(&token, lender_a.id()).await?;
    let balance_b_after = get_usdc_balance(&token, lender_b.id()).await?;
    assert_eq!(
        balance_a_after, initial_balance_a,
        "Lender A should receive their full refund"
    );
    assert_eq!(
        balance_b_after, initial_balance_b,
        "Lender B should receive their full refund"
    );

    Ok(())
}
