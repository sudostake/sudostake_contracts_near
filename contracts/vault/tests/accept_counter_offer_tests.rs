#![cfg(feature = "integration-test")]

use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use near_sdk::{json_types::U128, AccountId, NearToken};
use near_workspaces::network::Sandbox;
use near_workspaces::{Account, Contract, Worker};
use serde_json::json;
use test_utils::{
    create_test_validator, get_usdc_balance, initialize_test_token,
    initialize_test_vault_on_sub_account, make_counter_offer_msg, register_account_with_token,
    CounterOffer, LiquidityRequest, RefundEntry, VaultViewState, VAULT_CALL_GAS,
};

#[path = "test_lock.rs"]
mod test_lock;
#[path = "test_utils.rs"]
mod test_utils;

const REQUEST_AMOUNT: u128 = 1_000_000;
const REQUEST_INTEREST: u128 = 100_000;
const REQUEST_DURATION: u64 = 86_400;
const COLLATERAL_NEAR: u128 = 5;
const INITIAL_LENDER_BALANCE: u128 = 1_000_000;

struct TestEnv {
    /// Keep worker alive for the lifetime of the test.
    _worker: Worker<Sandbox>,
    root: Account,
    vault: Contract,
    token: Contract,
}

impl TestEnv {
    async fn new() -> Result<Self> {
        let worker = near_workspaces::sandbox().await?;
        let root = worker.root_account()?;
        let validator = create_test_validator(&worker, &root).await?;
        let token = initialize_test_token(&root).await?;
        let vault = initialize_test_vault_on_sub_account(&root).await?.contract;

        register_account_with_token(&root, &token, vault.id()).await?;

        root.call(vault.id(), "delegate")
            .args_json(json!({
                "validator": validator.id(),
                "amount": NearToken::from_near(COLLATERAL_NEAR),
            }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(VAULT_CALL_GAS)
            .transact()
            .await?
            .into_result()?;

        worker.fast_forward(1).await?;

        Ok(Self {
            _worker: worker,
            root,
            vault,
            token,
        })
    }

    async fn open_standard_request(&self) -> Result<LiquidityRequest> {
        self.root
            .call(self.vault.id(), "request_liquidity")
            .args_json(json!({
                "token": self.token.id(),
                "amount": U128(REQUEST_AMOUNT),
                "interest": U128(REQUEST_INTEREST),
                "collateral": NearToken::from_near(COLLATERAL_NEAR),
                "duration": REQUEST_DURATION
            }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(VAULT_CALL_GAS)
            .transact()
            .await?
            .into_result()?;

        self.vault_state()
            .await?
            .liquidity_request
            .ok_or_else(|| anyhow!("expected liquidity request to be recorded"))
    }

    async fn create_lender(&self, name: &str, tokens: u128) -> Result<Account> {
        let lender = self
            .root
            .create_subaccount(name)
            .initial_balance(NearToken::from_near(5))
            .transact()
            .await?
            .into_result()?;

        register_account_with_token(&self.root, &self.token, lender.id()).await?;

        self.root
            .call(self.token.id(), "ft_transfer")
            .args_json(json!({
                "receiver_id": lender.id(),
                "amount": tokens.to_string()
            }))
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await?
            .into_result()?;

        Ok(lender)
    }

    async fn submit_counter_offer(
        &self,
        lender: &Account,
        amount: u128,
        request: &LiquidityRequest,
    ) -> Result<()> {
        let msg = make_counter_offer_msg(request);

        lender
            .call(self.token.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": self.vault.id(),
                "amount": amount.to_string(),
                "msg": msg,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(VAULT_CALL_GAS)
            .transact()
            .await?
            .into_result()?;

        Ok(())
    }

    async fn vault_state(&self) -> Result<VaultViewState> {
        Ok(self.vault.view("get_vault_state").await?.json()?)
    }

    async fn counter_offers_map(&self) -> Result<Option<HashMap<String, CounterOffer>>> {
        Ok(self.vault.view("get_counter_offers").await?.json()?)
    }
}

#[tokio::test]
async fn accept_counter_offer_happy_path_updates_state() -> Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let env = TestEnv::new().await?;
    let request = env.open_standard_request().await?;

    let alice = env.create_lender("alice", INITIAL_LENDER_BALANCE).await?;
    let bob = env.create_lender("bob", INITIAL_LENDER_BALANCE).await?;
    let carol = env.create_lender("carol", INITIAL_LENDER_BALANCE).await?;

    env.submit_counter_offer(&alice, 800_000, &request).await?;
    env.submit_counter_offer(&bob, 850_000, &request).await?;
    env.submit_counter_offer(&carol, 900_000, &request).await?;

    let accepted = env
        .root
        .call(env.vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": carol.id(),
            "amount": U128(900_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let logs = accepted.logs();
    assert!(
        logs.iter().any(|log| {
            log.contains(r#""event":"counter_offer_accepted""#)
                && log.contains(carol.id().as_str())
                && log.contains(r#""accepted_amount":"900000""#)
        }),
        "expected acceptance log, got: {logs:?}"
    );

    let state = env.vault_state().await?;
    let updated_request = state
        .liquidity_request
        .context("liquidity request should remain present after acceptance")?;
    assert_eq!(
        updated_request.amount.0, 900_000,
        "principal should reflect accepted counter offer"
    );

    let accepted_offer = state
        .accepted_offer
        .context("accepted offer must be recorded")?;
    let lender = accepted_offer
        .get("lender")
        .and_then(|v| v.as_str())
        .context("accepted offer lender missing")?;
    assert_eq!(
        lender,
        carol.id().as_str(),
        "accepted offer lender should match proposer"
    );

    let counter_offers = env.counter_offers_map().await?;
    assert!(
        counter_offers.map(|m| m.is_empty()).unwrap_or(true),
        "counter offers map should be cleared"
    );

    let alice_balance = get_usdc_balance(&env.token, alice.id()).await?;
    let bob_balance = get_usdc_balance(&env.token, bob.id()).await?;
    let carol_balance = get_usdc_balance(&env.token, carol.id()).await?;
    assert_eq!(
        alice_balance.0, INITIAL_LENDER_BALANCE,
        "alice should receive a refund"
    );
    assert_eq!(
        bob_balance.0, INITIAL_LENDER_BALANCE,
        "bob should receive a refund"
    );
    assert_eq!(
        carol_balance.0,
        INITIAL_LENDER_BALANCE - 900_000,
        "accepted lender balance should decrease by the accepted amount"
    );

    let pending_refunds: Vec<(u64, RefundEntry)> = env
        .vault
        .view("get_refund_entries")
        .args_json(json!({ "account_id": null }))
        .await?
        .json()?;
    assert!(
        pending_refunds.is_empty(),
        "successful batch refund should leave no pending refund entries"
    );

    Ok(())
}

#[tokio::test]
async fn accept_counter_offer_records_failed_refund_entry_when_lender_unregistered() -> Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let env = TestEnv::new().await?;
    let request = env.open_standard_request().await?;

    let alice = env.create_lender("alice", INITIAL_LENDER_BALANCE).await?;
    let bob = env.create_lender("bob", INITIAL_LENDER_BALANCE).await?;
    let carol = env.create_lender("carol", INITIAL_LENDER_BALANCE).await?;

    env.submit_counter_offer(&alice, 800_000, &request).await?;
    env.submit_counter_offer(&bob, 850_000, &request).await?;
    env.submit_counter_offer(&carol, 900_000, &request).await?;

    bob.call(env.token.id(), "storage_unregister")
        .args_json(json!({ "force": true }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let outcome = env
        .root
        .call(env.vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": carol.id(),
            "amount": U128(900_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let logs = outcome.logs().join("\n");
    assert!(
        logs.contains(r#""event":"refund_failed""#),
        "Expected refund_failed log when refunding unregistered lender. Logs: {logs}"
    );

    let mut refunds: Vec<(u64, RefundEntry)> = env
        .vault
        .view("get_refund_entries")
        .args_json(json!({ "account_id": null }))
        .await?
        .json()?;
    refunds.sort_by(|a, b| a.1.proposer.cmp(&b.1.proposer));

    assert_eq!(
        refunds.len(),
        1,
        "Only the failed refund should remain in state after batch completion"
    );
    let failed_refund = &refunds[0].1;
    assert_eq!(
        failed_refund.proposer,
        bob.id().clone(),
        "Refund entry should belong to the unregistered lender"
    );
    assert_eq!(
        failed_refund.amount.0, 850_000,
        "Recorded refund amount should match the failed transfer"
    );

    let alice_balance = get_usdc_balance(&env.token, alice.id()).await?;
    assert_eq!(
        alice_balance.0, INITIAL_LENDER_BALANCE,
        "Registered lender should receive refund successfully"
    );

    let carol_balance = get_usdc_balance(&env.token, carol.id()).await?;
    assert_eq!(
        carol_balance.0,
        INITIAL_LENDER_BALANCE - 900_000,
        "Accepted lender balance should reflect loan amount"
    );

    Ok(())
}

#[tokio::test]
async fn accept_counter_offer_single_competing_offer_clears_refund_queue() -> Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let env = TestEnv::new().await?;
    let request = env.open_standard_request().await?;

    let bob = env
        .create_lender("solo-bob", INITIAL_LENDER_BALANCE)
        .await?;
    let carol = env
        .create_lender("solo-carol", INITIAL_LENDER_BALANCE)
        .await?;

    env.submit_counter_offer(&bob, 850_000, &request).await?;
    env.submit_counter_offer(&carol, 900_000, &request).await?;

    let outcome = env
        .root
        .call(env.vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": carol.id(),
            "amount": U128(900_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let logs = outcome.logs().join("\n");
    assert!(
        logs.contains(r#""event":"counter_offer_accepted""#),
        "Accept call should emit accepted log. Logs: {logs}"
    );

    let refunds: Vec<(u64, RefundEntry)> = env
        .vault
        .view("get_refund_entries")
        .args_json(json!({ "account_id": null }))
        .await?
        .json()?;
    assert!(
        refunds.is_empty(),
        "Single competing offer should be refunded via direct callback without lingering entries"
    );

    let bob_balance = get_usdc_balance(&env.token, bob.id()).await?;
    assert_eq!(
        bob_balance.0, INITIAL_LENDER_BALANCE,
        "Remaining lender should receive their refund"
    );

    Ok(())
}

#[tokio::test]
async fn accept_counter_offer_retry_clears_failed_refund_after_re_registration() -> Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let env = TestEnv::new().await?;
    let request = env.open_standard_request().await?;

    let alice = env
        .create_lender("retry-alice", INITIAL_LENDER_BALANCE)
        .await?;
    let bob = env
        .create_lender("retry-bob", INITIAL_LENDER_BALANCE)
        .await?;
    let carol = env
        .create_lender("retry-carol", INITIAL_LENDER_BALANCE)
        .await?;

    env.submit_counter_offer(&alice, 780_000, &request).await?;
    env.submit_counter_offer(&bob, 860_000, &request).await?;
    env.submit_counter_offer(&carol, 900_000, &request).await?;

    bob.call(env.token.id(), "storage_unregister")
        .args_json(json!({ "force": true }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    env.root
        .call(env.vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": carol.id(),
            "amount": U128(900_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let refunds_after_accept: Vec<(u64, RefundEntry)> = env
        .vault
        .view("get_refund_entries")
        .args_json(json!({ "account_id": null }))
        .await?
        .json()?;
    assert_eq!(
        refunds_after_accept.len(),
        1,
        "Failed refund should remain pending after initial acceptance"
    );

    register_account_with_token(&env.root, &env.token, bob.id()).await?;

    let retry_outcome = bob
        .call(env.vault.id(), "retry_refunds")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let retry_logs = retry_outcome.logs().join("\n");
    assert!(
        retry_logs.contains(r#""event":"retry_refund_succeeded""#),
        "Retry path should report success. Logs: {retry_logs}"
    );

    let refunds_after_retry: Vec<(u64, RefundEntry)> = env
        .vault
        .view("get_refund_entries")
        .args_json(json!({ "account_id": null }))
        .await?
        .json()?;
    assert!(
        refunds_after_retry.is_empty(),
        "Refund list should be cleared once retry succeeds"
    );

    let refund_amount = refunds_after_accept[0].1.amount.0;

    let bob_balance = get_usdc_balance(&env.token, bob.id()).await?;
    assert_eq!(
        bob_balance.0, refund_amount,
        "Lender balance should reflect the refunded amount after retry"
    );

    Ok(())
}

#[tokio::test]
async fn accept_counter_offer_requires_one_yocto() -> Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let env = TestEnv::new().await?;
    let request = env.open_standard_request().await?;

    let lender = env
        .create_lender("yocto-lender", INITIAL_LENDER_BALANCE)
        .await?;
    env.submit_counter_offer(&lender, 850_000, &request).await?;

    let outcome = env
        .root
        .call(env.vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": lender.id(),
            "amount": U128(850_000)
        }))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", outcome.failures());
    assert!(
        failure_text.contains("Requires attached deposit of exactly 1 yoctoNEAR"),
        "expected yocto guard failure, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn accept_counter_offer_rejects_non_owner() -> Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let env = TestEnv::new().await?;
    let request = env.open_standard_request().await?;

    let alice = env
        .create_lender("non-owner", INITIAL_LENDER_BALANCE)
        .await?;
    env.submit_counter_offer(&alice, 820_000, &request).await?;

    let outcome = alice
        .call(env.vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": alice.id(),
            "amount": U128(820_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", outcome.failures());
    assert!(
        failure_text.contains("Only the vault owner can accept a counter offer"),
        "expected owner guard failure, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn accept_counter_offer_rejects_missing_proposer() -> Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let env = TestEnv::new().await?;
    let request = env.open_standard_request().await?;

    let lender = env
        .create_lender("actual-lender", INITIAL_LENDER_BALANCE)
        .await?;
    env.submit_counter_offer(&lender, 870_000, &request).await?;

    let fake_proposer: AccountId = "fake.test.near".parse()?;
    let outcome = env
        .root
        .call(env.vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": fake_proposer,
            "amount": U128(870_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", outcome.failures());
    assert!(
        failure_text.contains("Counter offer from proposer not found"),
        "expected missing proposer failure, got: {failure_text}"
    );

    let offers = env
        .counter_offers_map()
        .await?
        .context("expected counter offer to remain intact")?;
    assert!(
        offers.contains_key(lender.id().as_str()),
        "existing counter offer should not be removed on failure"
    );

    Ok(())
}

#[tokio::test]
async fn accept_counter_offer_rejects_amount_mismatch() -> Result<()> {
    let _guard = test_lock::acquire_test_mutex().await;
    let env = TestEnv::new().await?;
    let request = env.open_standard_request().await?;

    let lender = env
        .create_lender("amount-lender", INITIAL_LENDER_BALANCE)
        .await?;
    env.submit_counter_offer(&lender, 860_000, &request).await?;

    let outcome = env
        .root
        .call(env.vault.id(), "accept_counter_offer")
        .args_json(json!({
            "proposer_id": lender.id(),
            "amount": U128(850_000)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    let failure_text = format!("{:?}", outcome.failures());
    assert!(
        failure_text.contains("Provided amount does not match the counter offer"),
        "expected amount mismatch failure, got: {failure_text}"
    );

    let balance_after = get_usdc_balance(&env.token, lender.id()).await?;
    assert_eq!(
        balance_after.0,
        INITIAL_LENDER_BALANCE - 860_000,
        "counter offer funds should remain locked after failed acceptance"
    );

    let state = env.vault_state().await?;
    assert!(
        state.accepted_offer.is_none(),
        "accepted offer should not be recorded on failure"
    );
    let offers = env
        .counter_offers_map()
        .await?
        .context("counter offer should still be present")?;
    assert!(
        offers.contains_key(lender.id().as_str()),
        "failed acceptance must not drop the counter offer"
    );

    Ok(())
}
