#![cfg(feature = "integration-test")]

use anyhow::Result;
use near_sdk::{json_types::U128, NearToken};
use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::{network::Sandbox, Account, Contract, Worker};
use serde_json::json;
use test_utils::{
    request_and_accept_liquidity, setup_contracts, setup_sandbox_and_accounts, UnstakeEntry,
    VaultViewState, VAULT_CALL_GAS, YOCTO_NEAR,
};

#[path = "test_utils.rs"]
mod test_utils;

const BLOCKS_PER_EPOCH: u64 = 500;
const MATURITY_PADDING_EPOCHS: u64 = 5;
const PAST_TIMESTAMP: u64 = 1_000_000_000;

mod helpers {
    use super::*;

    pub async fn delegate(
        root: &Account,
        vault: &Contract,
        validator: &Contract,
        amount: NearToken,
    ) -> Result<()> {
        root.call(vault.id(), "delegate")
            .args_json(json!({ "validator": validator.id(), "amount": amount }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(VAULT_CALL_GAS)
            .transact()
            .await?
            .into_result()?;
        Ok(())
    }

    pub async fn undelegate(
        root: &Account,
        vault: &Contract,
        validator: &Contract,
        amount: NearToken,
    ) -> Result<()> {
        root.call(vault.id(), "undelegate")
            .args_json(json!({ "validator": validator.id(), "amount": amount }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(VAULT_CALL_GAS)
            .transact()
            .await?
            .into_result()?;
        Ok(())
    }

    pub async fn withdraw_near(root: &Account, vault: &Contract, amount: u128) -> Result<()> {
        if amount == 0 {
            return Ok(());
        }

        root.call(vault.id(), "withdraw_balance")
            .args_json(json!({
                "token_address": null,
                "amount": amount.to_string(),
                "to": root.id()
            }))
            .deposit(NearToken::from_yoctonear(1))
            .gas(VAULT_CALL_GAS)
            .transact()
            .await?
            .into_result()?;
        Ok(())
    }

    pub async fn available_balance(vault: &Contract) -> Result<u128> {
        let balance: U128 = vault.view("view_available_balance").await?.json()?;
        Ok(balance.0)
    }

    pub async fn fast_forward_epochs(worker: &Worker<Sandbox>, epochs: u64) -> Result<()> {
        if epochs == 0 {
            return Ok(());
        }
        worker.fast_forward(epochs * BLOCKS_PER_EPOCH).await?;
        Ok(())
    }

    pub async fn force_offer_expired(vault: &Contract) -> Result<()> {
        vault
            .call("set_accepted_offer_timestamp")
            .args_json(json!({ "timestamp": PAST_TIMESTAMP }))
            .transact()
            .await?
            .into_result()?;
        Ok(())
    }

    pub async fn process_claims(
        caller: &Account,
        vault: &Contract,
    ) -> Result<ExecutionFinalResult> {
        let result = caller
            .call(vault.id(), "process_claims")
            .deposit(NearToken::from_yoctonear(1))
            .gas(VAULT_CALL_GAS)
            .transact()
            .await?;
        Ok(result)
    }

    pub fn find_unstake_amount(
        entries: &[(String, UnstakeEntry)],
        validator_id: &str,
    ) -> Option<u128> {
        entries
            .iter()
            .find(|(id, _)| id == validator_id)
            .map(|(_, entry)| entry.amount)
    }
}

#[tokio::test]
async fn process_claims_transfers_entire_debt_when_balance_sufficient() -> Result<()> {
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    helpers::delegate(&root, &vault, &validator, NearToken::from_near(5)).await?;
    worker.fast_forward(1).await?;

    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;
    helpers::force_offer_expired(&vault).await?;

    let result = helpers::process_claims(&lender, &vault)
        .await?
        .into_result()?;
    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains(r#""event":"liquidation_complete""#)),
        "Expected liquidation_complete event; logs: {:#?}",
        result.logs()
    );

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_none(),
        "Expected liquidity_request to be cleared"
    );
    assert!(
        state.accepted_offer.is_none(),
        "Expected accepted_offer to be cleared"
    );
    assert!(
        state.liquidation.is_none(),
        "Expected liquidation snapshot to be cleared"
    );

    Ok(())
}

#[tokio::test]
async fn process_claims_unstakes_full_shortfall_when_no_liquid_funds() -> Result<()> {
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    helpers::delegate(&root, &vault, &validator, NearToken::from_near(5)).await?;
    worker.fast_forward(1).await?;

    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    let available = helpers::available_balance(&vault).await?;
    helpers::withdraw_near(&root, &vault, available).await?;

    helpers::force_offer_expired(&vault).await?;
    let result = helpers::process_claims(&lender, &vault)
        .await?
        .into_result()?;
    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains("unstake_recorded")),
        "Expected unstake_recorded log; logs: {:#?}",
        result.logs()
    );

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_some(),
        "Liquidity request should remain open after scheduling unstake"
    );
    assert!(
        state.accepted_offer.is_some(),
        "Accepted offer should remain until repayment completes"
    );

    let validator_id = validator.id().to_string();
    let scheduled = helpers::find_unstake_amount(&state.unstake_entries, &validator_id)
        .expect("Expected unstake entry for validator");
    assert_eq!(
        scheduled / YOCTO_NEAR,
        5,
        "Expected approximately 5 NEAR scheduled to unstake"
    );
    assert!(
        state.active_validators.iter().all(|id| id != &validator_id),
        "Validator should be removed from active set when fully unstaked"
    );

    Ok(())
}

#[tokio::test]
async fn process_claims_combines_liquid_payout_with_unstake_when_partial_balance() -> Result<()> {
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    helpers::delegate(&root, &vault, &validator, NearToken::from_near(5)).await?;
    worker.fast_forward(1).await?;

    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    let available = helpers::available_balance(&vault).await?;
    let keep = NearToken::from_near(2).as_yoctonear();
    helpers::withdraw_near(&root, &vault, available.saturating_sub(keep)).await?;

    helpers::force_offer_expired(&vault).await?;
    let result = helpers::process_claims(&lender, &vault)
        .await?
        .into_result()?;

    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains(r#""event":"lender_payout_succeeded""#)),
        "Expected lender payout log; logs: {:#?}",
        result.logs()
    );

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_some(),
        "Loan should remain active after partial repayment"
    );
    assert!(
        state.accepted_offer.is_some(),
        "Accepted offer should remain active after partial repayment"
    );

    let validator_id = validator.id().to_string();
    let scheduled = helpers::find_unstake_amount(&state.unstake_entries, &validator_id)
        .expect("Expected unstake entry for validator");
    assert_eq!(
        scheduled / YOCTO_NEAR,
        3,
        "Expected approximately 3 NEAR to be unstaking after partial payout"
    );

    let remaining = helpers::available_balance(&vault).await?;
    assert!(
        remaining < 10u128.pow(20),
        "Expected vault liquid balance to be drained after payout, found {}",
        remaining
    );

    Ok(())
}

#[tokio::test]
async fn process_claims_waits_for_maturing_unstake_when_deficit_already_inflight() -> Result<()> {
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    helpers::delegate(&root, &vault, &validator, NearToken::from_near(10)).await?;
    worker.fast_forward(1).await?;
    helpers::undelegate(&root, &vault, &validator, NearToken::from_near(5)).await?;

    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    let available = helpers::available_balance(&vault).await?;
    helpers::withdraw_near(&root, &vault, available).await?;

    helpers::force_offer_expired(&vault).await?;
    let result = helpers::process_claims(&lender, &vault)
        .await?
        .into_result()?;
    assert!(
        result.logs().iter().any(|log| {
            log.contains(r#""event":"liquidation_progress""#)
                && log.contains(r#""reason":"NEAR unstaking""#)
        }),
        "Expected liquidation_progress waiting for NEAR unstaking; logs: {:#?}",
        result.logs()
    );

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let validator_id = validator.id().to_string();
    let scheduled = helpers::find_unstake_amount(&state.unstake_entries, &validator_id)
        .expect("Expected maturing unstake entry");
    assert_eq!(
        scheduled / YOCTO_NEAR,
        5,
        "Expected existing 5 NEAR unstake entry to remain in place"
    );
    assert!(
        state.liquidity_request.is_some() && state.accepted_offer.is_some(),
        "Loan should continue waiting on maturing stake"
    );

    Ok(())
}

#[tokio::test]
async fn process_claims_claims_matured_then_unstakes_remaining_shortfall() -> Result<()> {
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    helpers::delegate(&root, &vault, &validator, NearToken::from_near(8)).await?;
    worker.fast_forward(1).await?;
    helpers::undelegate(&root, &vault, &validator, NearToken::from_near(3)).await?;
    helpers::fast_forward_epochs(&worker, MATURITY_PADDING_EPOCHS).await?;

    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    let available = helpers::available_balance(&vault).await?;
    helpers::withdraw_near(&root, &vault, available).await?;

    helpers::force_offer_expired(&vault).await?;
    let result = helpers::process_claims(&lender, &vault)
        .await?
        .into_result()?;
    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains("unstake_recorded")),
        "Expected fallback unstake to be scheduled; logs: {:#?}",
        result.logs()
    );
    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains(r#""event":"lender_payout_succeeded""#)),
        "Expected lender payout log; logs: {:#?}",
        result.logs()
    );

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    let validator_id = validator.id().to_string();
    let scheduled = helpers::find_unstake_amount(&state.unstake_entries, &validator_id)
        .expect("Expected new unstake entry for remaining shortfall");
    assert!(
        scheduled >= YOCTO_NEAR && scheduled <= 3 * YOCTO_NEAR,
        "Expected remaining unstake between 1 and 3 NEAR, got {} yocto",
        scheduled
    );
    assert!(
        state.liquidity_request.is_some() && state.accepted_offer.is_some(),
        "Loan should remain active until remaining unstake settles"
    );

    Ok(())
}

#[tokio::test]
async fn process_claims_claims_matured_balance_and_completes_liquidation() -> Result<()> {
    let (worker, root, lender) = setup_sandbox_and_accounts().await?;
    let (validator, token, vault) = setup_contracts(&worker, &root, &lender).await?;

    helpers::delegate(&root, &vault, &validator, NearToken::from_near(10)).await?;
    worker.fast_forward(1).await?;
    helpers::undelegate(&root, &vault, &validator, NearToken::from_near(5)).await?;
    helpers::fast_forward_epochs(&worker, MATURITY_PADDING_EPOCHS).await?;

    request_and_accept_liquidity(&root, &lender, &vault, &token).await?;

    let available = helpers::available_balance(&vault).await?;
    helpers::withdraw_near(&root, &vault, available).await?;

    helpers::force_offer_expired(&vault).await?;
    let result = helpers::process_claims(&lender, &vault)
        .await?
        .into_result()?;
    assert!(
        result
            .logs()
            .iter()
            .any(|log| log.contains(r#""event":"liquidation_complete""#)),
        "Expected liquidation_complete event; logs: {:#?}",
        result.logs()
    );
    assert!(
        result.logs().iter().any(|log| {
            log.contains("lender_payout_succeeded") && log.contains("5000000000000000000000000")
        }),
        "Expected payout of 5 NEAR; logs: {:#?}",
        result.logs()
    );

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.liquidity_request.is_none(),
        "Expected liquidity_request to be cleared once fully repaid"
    );
    assert!(
        state.accepted_offer.is_none(),
        "Expected accepted_offer to be cleared once fully repaid"
    );
    assert!(
        state.liquidation.is_none(),
        "Liquidation state should be cleared after completion"
    );
    assert!(
        helpers::find_unstake_amount(&state.unstake_entries, &validator.id().to_string()).is_none(),
        "No unstake entries should remain once liquidation completes"
    );

    Ok(())
}
