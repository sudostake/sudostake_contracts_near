#![allow(dead_code)]

//! Liquidation & repayment pipeline for `Vault`.
//!
//! The functions in this module are intentionally ordered to mirror the control
//! flow:
//! 1. Public entry (`process_claims`)
//! 2. Synchronous driver (`next_liquidation_step`)
//! 3. Asynchronous callbacks (`on_*`)
//! 4. Helper routines
//!
//! Every asynchronous handler returns a `Promise`, which either hands control
//! back to the driver or finalises the operation.

use crate::{
    contract::{Vault, VaultExt},
    log_event,
    types::{
        ProcessingState, GAS_FOR_CALLBACK, GAS_FOR_LENDER_PAYOUT, GAS_FOR_UNSTAKE,
        GAS_FOR_VIEW_CALL, GAS_FOR_WITHDRAW_ALL, MAX_ACTIVE_VALIDATORS, NANOS_PER_SECOND,
        NUM_EPOCHS_TO_UNLOCK,
    },
};
use near_sdk::{
    assert_one_yocto, env, json_types::U128, near_bindgen, require, AccountId, Gas, NearToken,
    Promise, PromiseResult,
};

/// # Gas Budget Overview (`MAX_ACTIVE_VALIDATORS = 2`)
///
/// ## Steps
/// 1. **Step A – Matured withdrawal** (`next_liquidation_step` → `batch_claim_unstaked`)
///    - External work: `MAX_ACTIVE_VALIDATORS × GAS_FOR_WITHDRAW_ALL = 2 × 35 = 70 Tgas`
///    - Callback allocation: `GAS_FOR_CALLBACK_ON_BATCH_CLAIM_UNSTAKED = 180 Tgas`
///    - Peak: `70 + 180 = 250 Tgas`
/// 2. **Step B – Query total staked** (invoked directly or via `on_batch_claim_unstaked`)
///    - External work: `MAX_ACTIVE_VALIDATORS × GAS_FOR_VIEW_CALL = 2 × 10 = 20 Tgas`
///    - Callback allocation: `GAS_FOR_CALLBACK_ON_TOTAL_STAKED_PROCESS_CLAIMS = 140 Tgas`
///    - Peak: `20 + 140 = 160 Tgas` (covers Steps C & D)
/// 3. **Step C – Schedule unstake** (`on_total_staked_process_claims` → `batch_unstake`)
///    - Consumes `GAS_TGAS_BATCH_UNSTAKE = 2 × GAS_FOR_UNSTAKE = 60 Tgas` from Step B’s callback budget
/// 4. **Step D – Complete unstake & attempt payout** (`on_batch_unstake`)
///    - Consumes `GAS_TGAS_ON_BATCH_UNSTAKE = GAS_FOR_CALLBACK + GAS_FOR_LENDER_PAYOUT = 20 + 40 = 60 Tgas`
///      (already budgeted in Step B)
/// 5. **Step P – Direct payout** (driver-issued transfer)
///    - Allocation: `GAS_FOR_LENDER_PAYOUT = 40 Tgas` (invoked from the driver or indirectly via Step D)
///
/// ## Driver Scenarios
/// - **Case 1 – zero liquid balance, no matured unstake**
///   1. Step B executes (160 Tgas).
///   2. Steps C & D run within the Step B callback (no extra gas beyond the 140 Tgas allocation).
///   - Peak: **160 Tgas**.
/// - **Case 2 – no matured/maturing balance, liquid NEAR < remaining debt**
///   1. Step B executes (`20 + 140 = 160 Tgas`).
///   2. Steps C & D consume the callback budget; Step P fires afterwards (40 Tgas) in its own promise.
///   - Peak: **160 Tgas**.
/// - **Case 3 – zero liquid balance, enough maturing unstake**
///   1. Driver waits (`wait_for_unstake_progress`); no external calls.
///   - Peak: **≈0 Tgas**.
/// - **Case 4 – zero liquid balance, matured unstake available, liquid NEAR after claims < remaining debt**
///   1. Step A executes (`70 + 180 = 250 Tgas`).
///   2. Second hop follows Case 2 (160 Tgas).
///   - Peaks: **250 Tgas** then **160 Tgas**.
/// - **Case 5 – liquid balance already covers remaining debt**
///   1. Driver calls Step P immediately (40 Tgas).
///   - Peak: **40 Tgas**.
/// - **Case 6 – matured unstake covers remaining debt in one cycle**
///   1. Step A executes (`70 + 180 = 250 Tgas`).
///   2. Callback observes full coverage and triggers Step P (40 Tgas) without querying validators.
///   - Peaks: **250 Tgas** then **40 Tgas**.
/// Common allocation used by all callbacks for lightweight bookkeeping.
const GAS_TGAS_CALLBACK_OVERHEAD: u64 = GAS_FOR_CALLBACK.as_tgas();
/// Gas forwarded when paying the lender via `transfer_to_lender`.
const GAS_TGAS_LENDER_PAYOUT: u64 = GAS_FOR_LENDER_PAYOUT.as_tgas();
/// Upper bound for withdrawing matured stake across active validators.
const GAS_TGAS_WITHDRAW_MATURED: u64 = GAS_FOR_WITHDRAW_ALL.as_tgas() * MAX_ACTIVE_VALIDATORS;
/// Upper bound for querying each validator's staked balance.
const GAS_TGAS_VIEW_BALANCE_QUERIES: u64 = GAS_FOR_VIEW_CALL.as_tgas() * MAX_ACTIVE_VALIDATORS;
/// Upper bound for issuing unstake instructions to every validator.
const GAS_TGAS_BATCH_UNSTAKE: u64 = GAS_FOR_UNSTAKE.as_tgas() * MAX_ACTIVE_VALIDATORS;

/// Total callback gas reserved for `on_batch_unstake`, including the lender payout (Step D + Step P).
const GAS_TGAS_ON_BATCH_UNSTAKE: u64 = GAS_TGAS_CALLBACK_OVERHEAD + GAS_TGAS_LENDER_PAYOUT;
const GAS_FOR_CALLBACK_ON_BATCH_UNSTAKE: Gas = Gas::from_tgas(GAS_TGAS_ON_BATCH_UNSTAKE);

/// Budget for `on_total_staked_process_claims` and everything it triggers (`batch_unstake` + payout).
const GAS_TGAS_ON_TOTAL_STAKED_PROCESS_CLAIMS: u64 =
    GAS_TGAS_CALLBACK_OVERHEAD + GAS_TGAS_BATCH_UNSTAKE + GAS_TGAS_ON_BATCH_UNSTAKE;
const GAS_FOR_CALLBACK_ON_TOTAL_STAKED_PROCESS_CLAIMS: Gas =
    Gas::from_tgas(GAS_TGAS_ON_TOTAL_STAKED_PROCESS_CLAIMS);

/// Combined allocation for `on_batch_claim_unstaked`, the staking queries, and the downstream payout path.
const GAS_TGAS_ON_BATCH_CLAIM_UNSTAKED: u64 = GAS_TGAS_CALLBACK_OVERHEAD
    + GAS_TGAS_VIEW_BALANCE_QUERIES
    + GAS_TGAS_ON_TOTAL_STAKED_PROCESS_CLAIMS;
const GAS_FOR_CALLBACK_ON_BATCH_CLAIM_UNSTAKED: Gas =
    Gas::from_tgas(GAS_TGAS_ON_BATCH_CLAIM_UNSTAKED);

/// Log string used when waiting for already-requested unstake operations to mature.
const WAITING_REASON_UNSTAKING: &str = "NEAR unstaking";
/// Log string used when no additional stake can be reclaimed immediately.
const WAITING_REASON_NO_STAKE: &str = "no staked NEAR available to unstake";

/// Snapshot of unstake-related information used by the liquidation driver.
#[derive(Debug, Clone)]
struct UnstakeStats {
    /// Validators with balance ready to be withdrawn immediately.
    matured_validators: Vec<AccountId>,
    /// Total maturing balance across validators that still need to settle.
    maturing_total: u128,
    /// Outstanding loan amount that must be repaid to the lender.
    remaining_debt: u128,
}

impl UnstakeStats {
    /// Returns true if any validator already has matured unstaked balance.
    fn has_matured_unstaked(&self) -> bool {
        !self.matured_validators.is_empty()
    }

    /// Returns true if the amount currently maturing is already sufficient.
    fn has_enough_maturing_balance(&self) -> bool {
        self.maturing_total >= self.remaining_debt
    }
}

#[near_bindgen]
impl Vault {
    /// Public entry point used by the lender (or anyone) once the loan expires.
    ///
    /// Initialises liquidation (if needed), acquires the processing lock and
    /// hands control to the driver.
    #[payable]
    pub fn process_claims(&mut self) -> Promise {
        assert_one_yocto();
        self.ensure_liquidation_ready();
        self.acquire_processing_lock(ProcessingState::ProcessClaims);
        self.next_liquidation_step()
    }

    /// Lazily initialises the liquidation state and logs the transition.
    fn ensure_liquidation_ready(&mut self) {
        // Nothing to do if liquidation state was already initialised.
        if self.liquidation.is_some() {
            return;
        }

        let offer = self
            .accepted_offer
            .as_ref()
            .expect("No accepted offer found");

        let request = self
            .liquidity_request
            .as_ref()
            .expect("liquidity_request missing despite accepted offer");

        let expiration = offer.accepted_at + request.duration * NANOS_PER_SECOND;
        let now = env::block_timestamp();

        require!(
            now >= expiration,
            format!("Liquidation not allowed until {} (now {})", expiration, now)
        );

        self.liquidation = Some(crate::types::Liquidation {
            liquidated: NearToken::from_yoctonear(0),
        });

        log_event!(
            "liquidation_started",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "lender": offer.lender,
                "at": now.to_string()
            })
        );
    }

    /// Computes the next action that progresses the liquidation.
    /// Order: reuse matured funds first, then query for additional stake, fall back to payouts,
    /// waiting, or scheduling further unstake work.
    fn next_liquidation_step(&mut self) -> Promise {
        let lender = self.lender_account();
        let stats = self.snapshot_unstake_stats();

        if let Some(promise) = self.try_claim_matured_unstake(&stats) {
            return promise;
        }

        let available = self.get_available_balance().as_yoctonear();
        if available > 0 && available < stats.remaining_debt {
            let shortfall = stats.remaining_debt - available;
            if stats.maturing_total < shortfall {
                return self.query_additional_unstaking();
            }
        }

        if let Some(promise) = self.try_payout_liquid_balance(&lender, available) {
            return promise;
        }

        if stats.has_enough_maturing_balance() {
            return self.wait_for_unstake_progress(WAITING_REASON_UNSTAKING);
        }

        self.query_additional_unstaking()
    }

    /// Helper that returns the lender address associated with the accepted offer.
    fn lender_account(&self) -> AccountId {
        self.accepted_offer
            .as_ref()
            .expect("lender_account requires accepted offer")
            .lender
            .clone()
    }

    /// Prefer claiming matured unstake entries so they can be reused quickly.
    fn try_claim_matured_unstake(&mut self, stats: &UnstakeStats) -> Option<Promise> {
        if !stats.has_matured_unstaked() {
            return None;
        }

        let matured = stats.matured_validators.clone();
        let callback = Self::ext(env::current_account_id())
            .with_static_gas(GAS_FOR_CALLBACK_ON_BATCH_CLAIM_UNSTAKED)
            .on_batch_claim_unstaked(matured.clone());

        Some(self.batch_claim_unstaked(matured, callback))
    }

    /// If there is liquid NEAR available, forward it to the lender immediately.
    fn try_payout_liquid_balance(
        &mut self,
        lender: &AccountId,
        available: u128,
    ) -> Option<Promise> {
        if available == 0 {
            return None;
        }

        let outstanding = self.remaining_debt();
        let payout = outstanding.min(available);
        let finalize = payout == outstanding;

        let lender_id = lender.clone();
        Some(
            Promise::new(lender_id.clone())
                .transfer(NearToken::from_yoctonear(payout))
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(GAS_FOR_LENDER_PAYOUT)
                        .on_lender_payout_complete(lender_id, payout, finalize),
                ),
        )
    }

    /// When sufficient funds are already maturing we wait for the unlock.
    fn wait_for_unstake_progress(&mut self, reason: &str) -> Promise {
        log_event!(
            "liquidation_progress",
            near_sdk::serde_json::json!({ "status": "waiting", "reason": reason })
        );

        self.release_processing_lock();
        Promise::new(env::current_account_id())
    }

    /// Query validators to figure out how much more needs to be unstaked.
    fn query_additional_unstaking(&mut self) -> Promise {
        let validators = self.get_ordered_validator_list();
        let callback = Self::ext(env::current_account_id())
            .with_static_gas(GAS_FOR_CALLBACK_ON_TOTAL_STAKED_PROCESS_CLAIMS)
            .on_total_staked_process_claims(validators.clone());

        self.batch_query_total_staked(&validators, callback)
    }

    /// Handles the result of `batch_claim_unstaked`.
    /// Callback for the lender payout promise. Logs the outcome, optionally
    /// records a refund, and either finalises the liquidation or releases the
    /// processing lock so a new cycle can be started.
    #[private]
    pub fn on_lender_payout_complete(
        &mut self,
        lender: AccountId,
        amount: u128,
        finalize: bool,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) -> Promise {
        self.log_gas_checkpoint("on_lender_payout_complete");

        let success = result.is_ok();
        let vault_id = env::current_account_id();

        log_event!(
            if success {
                "lender_payout_succeeded"
            } else {
                "lender_payout_failed"
            },
            near_sdk::serde_json::json!({
                "vault": vault_id.clone(),
                "lender": lender.clone(),
                "amount": amount.to_string()
            })
        );

        if success {
            let total_liquidated = {
                let liquidation = self
                    .liquidation
                    .as_mut()
                    .expect("liquidation state missing despite successful payout");

                liquidation.liquidated = liquidation
                    .liquidated
                    .saturating_add(NearToken::from_yoctonear(amount));

                liquidation.liquidated.as_yoctonear()
            };

            if finalize {
                self.clear_liquidation_state();
                log_event!(
                    "liquidation_complete",
                    near_sdk::serde_json::json!({
                        "vault": vault_id.clone(),
                        "lender": lender,
                        "total_repaid": total_liquidated.to_string(),
                        "payout_status": "transferred"
                    })
                );
            }
        }

        self.release_processing_lock();
        Promise::new(vault_id)
    }

    #[private]
    pub fn on_batch_claim_unstaked(&mut self, validators: Vec<AccountId>) -> Promise {
        self.log_gas_checkpoint("on_batch_claim_unstaked");

        for (idx, validator) in validators.iter().enumerate() {
            match env::promise_result(idx as u64) {
                PromiseResult::Successful(_) => {
                    self.unstake_entries.remove(validator);
                }

                _ => env::log_str(&format!(
                    "Warning: withdraw_all() failed for validator {}",
                    validator
                )),
            }
        }

        self.next_liquidation_step()
    }

    /// Processes the response from `batch_query_total_staked`, computing the
    /// amounts that still need to be unstaked.
    #[private]
    pub fn on_total_staked_process_claims(
        &mut self,
        validator_ids: Vec<AccountId>,
    ) -> Promise {
        self.log_gas_checkpoint("on_total_staked_process_claims");

        let available = self.get_available_balance().as_yoctonear();
        let maturing_total = self.total_maturing_unstake_balance();
        let mut deficit = self
            .remaining_debt()
            .saturating_sub(available)
            .saturating_sub(maturing_total);

        if deficit == 0 {
            return self.wait_for_unstake_progress(WAITING_REASON_UNSTAKING);
        }

        let mut instructions: Vec<(AccountId, u128, bool)> = Vec::new();

        for (idx, validator_id) in validator_ids.into_iter().enumerate() {
            if deficit == 0 {
                break;
            }

            let Some(staked) = self.staked_balance_from_result(idx as u64, &validator_id) else {
                continue;
            };

            if staked == 0 {
                self.active_validators.remove(&validator_id);
                continue;
            }

            let amount = staked.min(deficit);
            if amount == 0 {
                continue;
            }

            instructions.push((validator_id.clone(), amount, amount == staked));
            deficit -= amount;
        }

        if instructions.is_empty() {
            return self.wait_for_unstake_progress(WAITING_REASON_NO_STAKE);
        }

        let callback = Self::ext(env::current_account_id())
            .with_static_gas(GAS_FOR_CALLBACK_ON_BATCH_UNSTAKE)
            .on_batch_unstake(instructions.clone());
        self.batch_unstake(instructions, callback)
    }

    fn staked_balance_from_result(
        &self,
        promise_index: u64,
        validator: &AccountId,
    ) -> Option<u128> {
        match env::promise_result(promise_index) {
            PromiseResult::Successful(bytes) => {
                match near_sdk::serde_json::from_slice::<U128>(&bytes) {
                    Ok(U128(amount)) => Some(amount),
                    Err(_) => {
                        env::log_str(&format!(
                            "Warning: staked balance response could not be parsed for validator {}",
                            validator
                        ));
                        None
                    }
                }
            }

            _ => {
                env::log_str(&format!(
                    "Warning: staked balance query failed for validator {}",
                    validator
                ));
                None
            }
        }
    }

    /// Handles completion of the batch unstake and opportunistically pays the lender.
    #[private]
    pub fn on_batch_unstake(&mut self, entries: Vec<(AccountId, u128, bool)>) -> Promise {
        self.log_gas_checkpoint("on_batch_unstake");

        for (idx, (validator, amount, removed_entire_stake)) in entries.into_iter().enumerate() {
            match env::promise_result(idx as u64) {
                PromiseResult::Successful(_) => {
                    if removed_entire_stake {
                        self.active_validators.remove(&validator);
                    }

                    self.update_validator_unstake_entry(&validator, amount);
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

        let lender = self.lender_account();
        let available = self.get_available_balance().as_yoctonear();
        if let Some(promise) = self.try_payout_liquid_balance(&lender, available) {
            return promise;
        }

        self.release_processing_lock();
        Promise::new(env::current_account_id())
    }
}

impl Vault {
    /// Sums the amount of NEAR currently waiting to unlock across validators.
    fn total_maturing_unstake_balance(&self) -> u128 {
        let current_epoch = env::epoch_height();
        let mut total = 0u128;

        for (_, entry) in self.unstake_entries.iter() {
            let maturity_epoch = entry.epoch_height.saturating_add(NUM_EPOCHS_TO_UNLOCK);
            if current_epoch < maturity_epoch {
                total += entry.amount;
            }
        }

        total
    }

    /// Builds a snapshot of matured and maturing unstake entries.
    fn snapshot_unstake_stats(&self) -> UnstakeStats {
        let current_epoch = env::epoch_height();
        let mut matured: Vec<AccountId> = Vec::new();
        let mut maturing_total = 0u128;

        for (validator, entry) in self.unstake_entries.iter() {
            let maturity_epoch = entry.epoch_height.saturating_add(NUM_EPOCHS_TO_UNLOCK);

            if current_epoch >= maturity_epoch {
                matured.push(validator);
            } else {
                maturing_total += entry.amount;
            }
        }

        UnstakeStats {
            matured_validators: matured,
            maturing_total,
            remaining_debt: self.remaining_debt(),
        }
    }

    fn remaining_debt(&self) -> u128 {
        let total = self
            .liquidity_request
            .as_ref()
            .expect("collateral requires active liquidity request")
            .collateral
            .as_yoctonear();

        total
            - self
                .liquidation
                .as_ref()
                .expect("remaining_debt requires liquidation state")
                .liquidated
                .as_yoctonear()
    }

    /// Removes all liquidation-related state from the vault.
    fn clear_liquidation_state(&mut self) {
        self.liquidity_request = None;
        self.accepted_offer = None;
        self.liquidation = None;
        self.pending_liquidity_request = None;
    }
}
