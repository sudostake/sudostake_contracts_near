#![allow(dead_code)]

// -----------------------------------------------------------------------------
//  process_claims.rs – Liquidation & repayment pipeline for `Vault`
// -----------------------------------------------------------------------------
//  This file orchestrates the end‑to‑end repayment flow once a liquidity request
//  has expired. The asynchronous state machine advances via the following phases:
//
//    1. `process_claims` (public entry)
//    2. `next_liquidation_step` (sync driver)
//    3. `on_batch_claim_unstaked`  ┐
//    4. `on_total_staked_process_claims` ┤ async callbacks
//    5. `on_batch_unstake`         ┘
//
//  Helper routines (locks, accounting, state inspection) live at the bottom of
//  the file to keep the high‑level flow readable.
// -----------------------------------------------------------------------------

use crate::{
    contract::{Vault, VaultExt},
    log_event,
    types::{ProcessingState, GAS_FOR_CALLBACK, NUM_EPOCHS_TO_UNLOCK},
};
use near_sdk::{
    assert_one_yocto, env, json_types::U128, near_bindgen, require, AccountId, Gas, NearToken,
    Promise, PromiseResult,
};

/**
* Worse case scenario for MAX_ACTIVE_VALIDATORS = 2
* Root - 20Tgas
  batch_claim_unstaked - (35 + 35)Tgas
  [20 + 70]Tgas
        ↳(200Tgas):on_batch_claim_unstaked - 20Tgas
                   batch_query_total_staked - (10 + 10)Tgas
                   [20 + 20]Tgas
                       ↳(160Tgas):on_total_staked_process_claims - 20 Tgas
                                  batch_unstake - (60 + 60) Tgas
                                       [20 + 120]Tgas
                                           ↳(20Tgas):on_batch_unstake - 20Tgas
                                                     [20]Tgas
*/
const GAS_FOR_CALLBACK_ON_BATCH_CLAIM_UNSTAKED: Gas = Gas::from_tgas(200);
const GAS_FOR_CALLBACK_ON_TOTAL_STAKED_PROCESS_CLAIMS: Gas = Gas::from_tgas(160);

#[near_bindgen]
impl Vault {
    /// Entry‑point triggered by the lender (or anyone) **after** the liquidity
    /// request has expired. It kicks off or continues the liquidation flow.
    ///
    /// Steps performed:
    /// 1. Access‑control (1 yocto)
    /// 2. Initialise liquidation if needed & fetch the lender account.
    /// 3. Guard against concurrent execution.
    /// 4. Transfer any liquid balance to the lender.
    /// 5. Hand off to the asynchronous state‑machine (`next_liquidation_step`).
    #[payable]
    pub fn process_claims(&mut self) -> Promise {
        assert_one_yocto();
        let lender = self.ensure_liquidation_ready();
        self.acquire_processing_lock(ProcessingState::ProcessClaims);
        self.process_repayment(&lender);
        self.next_liquidation_step()
    }

    // ---------------------------------------------------------
    // LIQUIDATION DRIVER
    // ---------------------------------------------------------
    /// Decide and schedule the next asynchronous action required to complete
    /// liquidation. Returns a no‑op promise when nothing else is required.
    fn next_liquidation_step(&mut self) -> Promise {
        // If fully repaid, nothing more to do.
        if self.liquidity_request.is_none() {
            self.release_processing_lock();
            return Promise::new(env::current_account_id());
        }

        // Get validators with matured unstaked balance, maturing_total and remaining_debt
        let (validators_with_matured_unstaked, maturing_total) = self.get_unstaked_entries_stats();
        let remaining_debt = self.remaining_debt();

        match (
            validators_with_matured_unstaked.is_empty(),
            maturing_total >= remaining_debt,
        ) {
            // (A) Immediate claim – some validators already have matured funds.
            (false, _) => {
                let cb = Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK_ON_BATCH_CLAIM_UNSTAKED)
                    .on_batch_claim_unstaked(
                        validators_with_matured_unstaked.clone(),
                        maturing_total,
                    );
                self.batch_claim_unstaked(validators_with_matured_unstaked, cb)
            }

            // (B) Nothing matured yet but sufficient funds are maturing.
            (true, true) => {
                self.log_waiting("NEAR unstaking");
                self.release_processing_lock();
                Promise::new(env::current_account_id())
            }

            // (C) Need to unstake additional NEAR.
            (true, false) => {
                let cb = Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK_ON_TOTAL_STAKED_PROCESS_CLAIMS)
                    .on_total_staked_process_claims(maturing_total);
                self.batch_query_total_staked(cb)
            }
        }
    }

    /// Callback after `batch_claim_unstaked`. Removes claimed entries, pays out
    /// newly liquid NEAR, and determines whether further action is needed.
    #[private]
    pub fn on_batch_claim_unstaked(
        &mut self,
        validators: Vec<AccountId>,
        maturing_total: u128,
    ) -> Promise {
        self.log_gas_checkpoint("on_batch_claim_unstaked");

        // Remove successfully claimed entries.
        for (idx, validator) in validators.iter().enumerate() {
            if matches!(
                env::promise_result(idx as u64),
                PromiseResult::Successful(_)
            ) {
                self.unstake_entries.remove(validator);
            } else {
                env::log_str(&format!(
                    "Warning: withdraw_all() failed for validator {}",
                    validator
                ));
            }
        }

        // Pay out freshly available balance.
        if let Some(lender) = self.accepted_offer.as_ref().map(|o| o.lender.clone()) {
            self.process_repayment(&lender);
        }

        // Decide if further action is required.
        if self.liquidity_request.is_some() {
            let remaining = self.remaining_debt();
            if maturing_total < remaining {
                let cb = Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK_ON_TOTAL_STAKED_PROCESS_CLAIMS)
                    .on_total_staked_process_claims(maturing_total);
                return self.batch_query_total_staked(cb);
            }
            self.log_waiting("NEAR unstaking");
        }

        //  Unlock & finish
        self.release_processing_lock();
        Promise::new(env::current_account_id())
    }

    /// Callback after `batch_query_total_staked`. Determines per‑validator
    /// unstake amounts required and triggers `batch_unstake`.
    #[private]
    pub fn on_total_staked_process_claims(&mut self, maturing_total: u128) {
        self.log_gas_checkpoint("on_total_staked_process_claims");

        // Compute how much needs to be unstaked
        let mut deficit = self.remaining_debt().saturating_sub(maturing_total);
        let mut unstake_instructions: Vec<(AccountId, u128)> = vec![];

        for (idx, validator) in self.active_validators.iter().enumerate() {
            if deficit == 0 {
                break;
            }

            match env::promise_result(idx as u64) {
                PromiseResult::Successful(bytes) => {
                    if let Ok(U128(staked)) = near_sdk::serde_json::from_slice::<U128>(&bytes) {
                        let amount = staked.min(deficit);
                        if amount > 0 {
                            unstake_instructions.push((validator.clone(), amount));
                            deficit -= amount;
                        }
                    }
                }
                _ => {
                    env::log_str(&format!(
                        "Warning: staked balance query failed for validator {}",
                        validator
                    ));
                }
            }
        }

        // If nothing to unstake, just unlock
        if unstake_instructions.is_empty() {
            self.release_processing_lock();
            self.log_waiting("no staked NEAR available to unstake");
            return;
        }

        // Issue batch_unstake
        let cb = Self::ext(env::current_account_id())
            .with_static_gas(GAS_FOR_CALLBACK)
            .on_batch_unstake(unstake_instructions.clone());
        self.batch_unstake(unstake_instructions, cb);
    }

    /// Callback after [`batch_unstake`]. Updates local state with the new
    /// `UnstakeEntry` records.
    #[private]
    pub fn on_batch_unstake(&mut self, entries: Vec<(AccountId, u128)>) {
        self.log_gas_checkpoint("on_batch_unstake");

        // Unlock processing claims
        self.release_processing_lock();

        // Iterate over each (validator, amount) entry
        for (idx, (validator, amount)) in entries.into_iter().enumerate() {
            match env::promise_result(idx as u64) {
                PromiseResult::Successful(_) => {
                    // Update unstake_entry for validator
                    self.update_validator_unstake_entry(&validator, amount);

                    // Log unstake_recorded event
                    log_event!(
                        "unstake_recorded",
                        near_sdk::serde_json::json!({
                            "vault": env::current_account_id(),
                            "validator": validator,
                            "amount": amount.to_string(),
                            "epoch_height": env::epoch_height().to_string()
                        })
                    );
                }
                _ => {
                    // Log failed unstake attempt
                    log_event!(
                        "unstake_failed",
                        near_sdk::serde_json::json!({
                            "vault": env::current_account_id(),
                            "validator": validator,
                            "amount": amount.to_string()
                        })
                    );
                }
            }
        }
    }
}

impl Vault {
    fn ensure_liquidation_ready(&mut self) -> AccountId {
        // Ensure there is an active option on this vault
        let offer = self
            .accepted_offer
            .as_ref()
            .expect("No accepted offer found");

        // Initialize liquidation if needed
        if self.liquidation.is_none() {
            let request = self.liquidity_request.as_ref().unwrap();
            let now = env::block_timestamp();
            let expiration = offer.accepted_at + (request.duration * 1_000_000_000);

            // Ensure option has expired
            require!(
                now >= expiration,
                format!("Liquidation not allowed until {} (now {})", expiration, now)
            );

            // Begin tracking liquidation
            self.liquidation = Some(crate::types::Liquidation {
                liquidated: NearToken::from_yoctonear(0),
            });

            // Log liquidation_started event
            log_event!(
                "liquidation_started",
                near_sdk::serde_json::json!({
                    "vault": env::current_account_id(),
                    "lender": offer.lender,
                    "at": now.to_string()
                })
            );
        }

        // Return the lender
        offer.lender.clone()
    }

    fn total_debt(&self) -> u128 {
        self.liquidity_request
            .as_ref()
            .unwrap()
            .collateral
            .as_yoctonear()
    }

    fn remaining_debt(&self) -> u128 {
        self.total_debt() - self.liquidation.as_ref().unwrap().liquidated.as_yoctonear()
    }

    fn get_unstaked_entries_stats(&self) -> (Vec<AccountId>, u128) {
        let current_epoch = env::epoch_height();
        let mut matured: Vec<AccountId> = vec![];
        let mut maturing_total = 0u128;

        for (validator, entry) in self.unstake_entries.iter() {
            if current_epoch >= entry.epoch_height + NUM_EPOCHS_TO_UNLOCK {
                matured.push(validator);
            } else {
                maturing_total += entry.amount;
            }
        }

        (matured, maturing_total)
    }

    fn transfer_to_lender(&mut self, lender: &AccountId, amount: u128) -> Promise {
        let liquidation = self.liquidation.as_mut().unwrap();
        liquidation.liquidated = liquidation
            .liquidated
            .saturating_add(NearToken::from_yoctonear(amount));

        Promise::new(lender.clone()).transfer(NearToken::from_yoctonear(amount))
    }

    fn clear_liquidation_state(&mut self) {
        self.liquidity_request = None;
        self.accepted_offer = None;
        self.liquidation = None;
        self.release_processing_lock();
    }

    fn finalize_liquidation(&mut self, lender: &AccountId, amount: u128) {
        self.transfer_to_lender(lender, amount);

        log_event!(
            "liquidation_complete",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "lender": lender,
                "total_repaid": self.total_debt().to_string()
            })
        );

        self.clear_liquidation_state();
    }

    fn process_repayment(&mut self, lender: &AccountId) {
        let outstanding = self.remaining_debt();
        let available = self.get_available_balance().as_yoctonear();

        if available >= outstanding {
            self.finalize_liquidation(lender, outstanding);
        } else if available > 0 {
            self.transfer_to_lender(lender, available);
        }
    }

    fn log_waiting(&self, reason: &str) {
        log_event!(
            "liquidation_progress",
            near_sdk::serde_json::json!({ "status": "waiting", "reason": reason })
        );
    }
}
