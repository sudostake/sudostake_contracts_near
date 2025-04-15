#![allow(dead_code)]

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::ext::ext_self;
use crate::ext::ext_staking_pool;
use crate::log_event;
use crate::types::UnstakeEntry;
use crate::types::{GAS_FOR_CALLBACK, GAS_FOR_UNSTAKE};
use near_sdk::json_types::U128;
use near_sdk::require;
use near_sdk::{assert_one_yocto, env, near_bindgen, AccountId, NearToken, Promise};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn undelegate(&mut self, validator: AccountId, amount: NearToken) -> Promise {
        // Require 1 yoctoNEAR for access control
        assert_one_yocto();

        // Must be the vault owner
        require!(
            env::predecessor_account_id() == self.owner,
            "Only the vault owner can undelegate"
        );

        // Amount must be greater than 0
        require!(amount.as_yoctonear() > 0, "Amount must be greater than 0");

        // Validator must be currently active
        require!(
            self.active_validators.contains(&validator),
            "Validator is not currently active"
        );

        // Proceed with unstaking the intended amount
        ext_staking_pool::ext(validator.clone())
            .with_static_gas(GAS_FOR_UNSTAKE)
            .unstake(U128::from(amount.as_yoctonear()))
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_unstake(validator, amount),
            )
    }

    #[private]
    pub fn on_unstake(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) {
        // Inspect amount of gas left
        self.log_gas_checkpoint("on_unstake");

        if result.is_err() {
            env::panic_str("Failed to execute unstake on validator");
        } else {
            log_event!(
                "undelegate_completed",
                near_sdk::serde_json::json!({
                    "validator": validator,
                    "amount": amount
                })
            );

            // Add or update unstake entry for validator
            if let Some(mut entry) = self.unstake_entries.get(&validator) {
                entry.amount += amount.as_yoctonear();
                entry.epoch_height = env::epoch_height();
                self.unstake_entries.insert(&validator, &entry);
            } else {
                self.unstake_entries.insert(
                    &validator,
                    &UnstakeEntry {
                        amount: amount.as_yoctonear(),
                        epoch_height: env::epoch_height(),
                    },
                );
            }
        }
    }
}
