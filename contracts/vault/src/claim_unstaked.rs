#![allow(dead_code)]

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::ext::ext_self;
use crate::ext::ext_staking_pool;
use crate::log_event;
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
        let entry = self
            .unstake_entries
            .get(&validator)
            .expect("No unstake entry found for validator");

        // Ensure the required number of epochs has passed
        let current_epoch = env::epoch_height();
        require!(
            current_epoch >= entry.epoch_height + NUM_EPOCHS_TO_UNLOCK,
            format!(
                "Unstaked funds not yet claimable (current_epoch: {}, required_epoch: {})",
                current_epoch,
                entry.epoch_height + NUM_EPOCHS_TO_UNLOCK
            )
        );

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

        if result.is_err() {
            log_event!(
                "claim_unstake_failed",
                near_sdk::serde_json::json!({
                    "validator": validator,
                    "error": "withdraw_all failed"
                })
            );

            // Throw an error
            env::panic_str("Failed to execute withdraw_all on validator");
        }

        // Log claim_unstaked_completed event
        log_event!(
            "claim_unstaked_completed",
            near_sdk::serde_json::json!({
                "validator": validator,
            })
        );

        // Clear unstake entry for valdator
        self.unstake_entries.remove(&validator);
    }
}
