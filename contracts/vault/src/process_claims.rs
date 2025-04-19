#![allow(dead_code)]

use crate::{
    contract::{Vault, VaultExt},
    ext::ext_staking_pool,
    log_event,
    types::{UnstakeEntry, GAS_FOR_CALLBACK},
};
use near_sdk::{
    assert_one_yocto, env, json_types::U128, near_bindgen, require, AccountId, Promise,
    PromiseResult,
};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn process_claims(&mut self) -> Promise {
        // Require 1 yoctoNEAR for access control
        assert_one_yocto();

        // Ensure the request has been accepted
        let offer = self
            .accepted_offer
            .as_ref()
            .expect("No accepted offer found");

        // Ensure option has expired
        let request = self.liquidity_request.as_ref().unwrap();
        let now = env::block_timestamp();

        // Initialize liquidation if needed
        if self.liquidation.is_none() {
            let expiration = offer.accepted_at + (request.duration * 1_000_000_000);
            require!(
                now >= expiration,
                format!("Liquidation not allowed until {} (now {})", expiration, now)
            );

            self.liquidation = Some(crate::types::Liquidation { liquidated: 0u128 });

            // Log liquidation_started event
            log_event!(
                "liquidation_started",
                near_sdk::serde_json::json!({
                    "lender": offer.lender,
                    "at": now.to_string()
                })
            );
        }

        // Process repayment with available balance
        self.process_repayment(offer.lender.clone());

        // If the liquidity request is still open, try to claim matured unstaked_entries
        // or unstake the outstanding balance
        if self.liquidity_request.is_some() {
            let matured = self.get_matured_unstaked_entries();
            if !matured.is_empty() {
                return self.batch_claim_unstaked(matured);
            } else {
                return self.batch_query_total_staked(
                    Self::ext(env::current_account_id())
                        .with_static_gas(GAS_FOR_CALLBACK)
                        .on_total_staked_process_claims(),
                );
            }
        }

        // Return a no-op Promise when no further liquidation steps are needed
        Promise::new(env::current_account_id())
    }

    #[private]
    pub fn on_batch_claim_unstaked(&mut self, validators: Vec<AccountId>) -> Promise {
        // Log gas checkpoint
        self.log_gas_checkpoint("on_batch_claim_unstaked");

        // Ensure the number of results matches the number of validators
        let num_results = env::promise_results_count();
        assert_eq!(
            num_results,
            validators.len() as u64,
            "Mismatch between validators and promise results"
        );

        // Remove only validators from unstake_entries if withdraw_all succeeds
        for (i, validator) in validators.iter().enumerate() {
            match env::promise_result(i as u64) {
                PromiseResult::Successful(_) => {
                    self.unstake_entries.remove(validator);
                }
                _ => {
                    // Log any failed withdraws
                    env::log_str(&format!(
                        "Warning: withdraw_all() failed for validator {}",
                        validator
                    ));
                }
            }
        }

        // Process repayment with available balance
        self.process_repayment(
            self.accepted_offer
                .as_ref()
                .expect("No accepted offer")
                .lender
                .clone(),
        );

        // If the liquidity request is still open, after claiming all unstaked balance
        // try to unstake the outstanding balance
        if self.liquidity_request.is_some() {
            return self.batch_query_total_staked(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_total_staked_process_claims(),
            );
        }

        // We return
        Promise::new(env::current_account_id())
    }

    #[private]
    pub fn on_total_staked_process_claims(&mut self) {
        // Log gas checkpoint
        self.log_gas_checkpoint("on_total_staked_process_claims");

        // Get active validators from internal state
        let validators: Vec<AccountId> = self.active_validators.iter().collect();

        // Ensure the number of results matches the number of validators
        let num_results = env::promise_results_count();
        assert_eq!(
            num_results,
            validators.len() as u64,
            "Mismatch between validators and promise results"
        );

        // Compute how much needs to be unstaked
        let mut to_unstake = self.get_remaining_debt();
        let mut unstake_instructions: Vec<(AccountId, u128)> = vec![];
        for (i, validator) in validators.iter().enumerate() {
            if to_unstake == 0 {
                break;
            }

            match env::promise_result(i as u64) {
                PromiseResult::Successful(bytes) => {
                    if let Ok(U128(staked)) = near_sdk::serde_json::from_slice::<U128>(&bytes) {
                        let amount = staked.min(to_unstake);
                        if amount > 0 {
                            unstake_instructions.push((validator.clone(), amount));
                            to_unstake -= amount;
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

        // If nothing to unstake, just log and return
        if unstake_instructions.is_empty() {
            log_event!(
                "liquidation_progress",
                near_sdk::serde_json::json!({ "status": "waiting", "reason": "no staked NEAR available to unstake" })
            );
            return;
        }

        // Build the batch unstake call chain
        let mut batch = ext_staking_pool::ext(unstake_instructions[0].0.clone())
            .with_static_gas(crate::types::GAS_FOR_UNSTAKE)
            .unstake(U128::from(unstake_instructions[0].1));
        for (validator, amount) in unstake_instructions.iter().skip(1) {
            batch = batch.and(
                ext_staking_pool::ext(validator.clone())
                    .with_static_gas(crate::types::GAS_FOR_UNSTAKE)
                    .unstake(U128::from(*amount)),
            );
        }

        // Callback for updating unstake_entries
        batch.then(
            Self::ext(env::current_account_id())
                .with_static_gas(crate::types::GAS_FOR_CALLBACK)
                .on_batch_unstake(unstake_instructions),
        );
    }

    #[private]
    pub fn on_batch_unstake(&mut self, entries: Vec<(AccountId, u128)>) {
        // Log remaining gas
        self.log_gas_checkpoint("on_batch_unstake");

        // Ensure we received a result for each unstake entry
        let result_count = env::promise_results_count();
        assert_eq!(
            result_count,
            entries.len() as u64,
            "Mismatch between promise results and entries passed in"
        );

        // Track the current epoch height
        let current_epoch = env::epoch_height();

        // Iterate over each (validator, amount) entry
        for (i, (validator, amount)) in entries.into_iter().enumerate() {
            match env::promise_result(i as u64) {
                PromiseResult::Successful(_) => {
                    // Get the validator unstake entry
                    let mut entry =
                        self.unstake_entries
                            .get(&validator)
                            .unwrap_or_else(|| UnstakeEntry {
                                amount: 0,
                                epoch_height: 0,
                            });

                    // Update the entry and save to state
                    entry.amount += amount;
                    entry.epoch_height = env::epoch_height();
                    self.unstake_entries.insert(&validator, &entry);

                    // Log unstake_recorded event
                    log_event!(
                        "unstake_recorded",
                        near_sdk::serde_json::json!({
                            "validator": validator,
                            "amount": amount.to_string(),
                            "epoch_height": current_epoch.to_string()
                        })
                    );
                }
                _ => {
                    // Log failed unstake attempt
                    log_event!(
                        "unstake_failed",
                        near_sdk::serde_json::json!({
                            "validator": validator,
                            "amount": amount.to_string()
                        })
                    );
                }
            }
        }
    }
}
