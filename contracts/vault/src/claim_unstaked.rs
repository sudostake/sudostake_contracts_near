#![allow(dead_code)]

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::ext::ext_self;
use crate::ext::ext_staking_pool;
use crate::log_event;
use crate::types::ProcessingState;
use crate::types::GAS_FOR_CALLBACK;
use crate::types::GAS_FOR_WITHDRAW_ALL;
use crate::types::NUM_EPOCHS_TO_UNLOCK;
use near_sdk::{assert_one_yocto, env, near_bindgen, require, AccountId, Promise};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn claim_unstaked(&mut self, validator: AccountId) -> Promise {
        // Require 1 yoctoNEAR for access control
        assert_one_yocto();

        // Only the vault owner can perform this action
        require!(
            env::predecessor_account_id() == self.owner,
            "Only the vault owner can claim unstaked balance"
        );

        // Get the unstake entry for this validator
        let entry = self.unstake_entries.get(&validator);
        require!(
            entry.is_some(),
            format!("No unstake entry found for validator {}", validator)
        );

        // Ensure the required number of epochs has passed
        let current_epoch = env::epoch_height();
        let entry = entry.unwrap();
        require!(
            current_epoch >= entry.epoch_height + NUM_EPOCHS_TO_UNLOCK,
            format!(
                "Unstaked funds not yet claimable (current_epoch: {}, required_epoch: {})",
                current_epoch,
                entry.epoch_height + NUM_EPOCHS_TO_UNLOCK
            )
        );

        // Prevent this action during liquidation
        require!(
            self.liquidation.is_none(),
            "Cannot claim unstaked NEAR while liquidation is in progress"
        );

        // Lock the vault for **ClaimUnstaked** workflow
        self.acquire_processing_lock(ProcessingState::ClaimUnstaked);

        // Trigger withdraw_all → then clear unstake_entries in the callback
        ext_staking_pool::ext(validator.clone())
            .with_static_gas(GAS_FOR_WITHDRAW_ALL)
            .withdraw_all()
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_withdraw_all(validator),
            )
    }

    #[private]
    pub fn on_withdraw_all(
        &mut self,
        validator: AccountId,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) {
        // Inspect amount of gas left
        self.log_gas_checkpoint("on_withdraw_all");
        self.release_processing_lock();

        if result.is_err() {
            log_event!(
                "claim_unstake_failed",
                near_sdk::serde_json::json!({
                    "vault": env::current_account_id(),
                    "validator": validator,
                    "error": "withdraw_all failed"
                })
            );

            return;
        }

        // Log claim_unstaked_completed event
        log_event!(
            "claim_unstaked_completed",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "validator": validator,
            })
        );

        // Clear unstake entry for valdator
        self.unstake_entries.remove(&validator);
    }
}
