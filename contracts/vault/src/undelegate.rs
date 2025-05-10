#![allow(dead_code)]

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::ext::ext_staking_pool;
use crate::log_event;
use crate::types::ProcessingState;
use crate::types::GAS_FOR_VIEW_CALL;
use crate::types::{GAS_FOR_CALLBACK, GAS_FOR_UNSTAKE};
use near_sdk::json_types::U128;
use near_sdk::{assert_one_yocto, env, near_bindgen, require, AccountId, Gas, NearToken, Promise};

const GAS_FOR_CALLBACK_ON_UNSTAKE_COMPLETE: Gas = Gas::from_tgas(200);

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

        // Disallow undelegation when a liquidity request is open
        require!(
            self.liquidity_request.is_none(),
            "Cannot undelegate when a liquidity request is open"
        );

        // Lock the vault for **Undelegate** workflow
        self.acquire_processing_lock(ProcessingState::Undelegate);

        // Proceed with unstaking the intended amount
        ext_staking_pool::ext(validator.clone())
            .with_static_gas(GAS_FOR_UNSTAKE)
            .unstake(U128::from(amount.as_yoctonear()))
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK_ON_UNSTAKE_COMPLETE)
                    .on_unstake_complete(validator, amount),
            )
    }

    #[private]
    pub fn on_unstake_complete(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) -> Promise {
        // Inspect amount of gas left
        self.log_gas_checkpoint("on_unstake_complete");

        if result.is_err() {
            log_event!(
                "undelegate_failed",
                near_sdk::serde_json::json!({
                    "vault": env::current_account_id(),
                    "validator": validator,
                    "amount": amount,
                    "error": "unstake failed"
                })
            );

            //  Unlock & finish
            self.release_processing_lock();
            return Promise::new(env::current_account_id());
        }

        // Log undelegate_completed event
        log_event!(
            "undelegate_completed",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "validator": validator,
                "amount": amount
            })
        );

        // Update unstake_entry for validator
        self.update_validator_unstake_entry(&validator, amount.as_yoctonear());

        // Proceed to check the total staked balance
        // remaining at the validator
        ext_staking_pool::ext(validator.clone())
            .with_static_gas(GAS_FOR_VIEW_CALL)
            .get_account_staked_balance(env::current_account_id())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_account_staked_balance(validator),
            )
    }

    #[private]
    pub fn on_account_staked_balance(
        &mut self,
        validator: AccountId,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    ) {
        self.log_gas_checkpoint("on_account_staked_balance");
        self.release_processing_lock();

        match result {
            Ok(balance) => {
                log_event!(
                    "staked_balance_callback_success",
                    near_sdk::serde_json::json!({
                        "vault": env::current_account_id(),
                        "validator": validator,
                        "staked_balance": balance
                    })
                );

                if balance.0 == 0 {
                    // Remove from active set
                    self.active_validators.remove(&validator);

                    // Log validator_removed event
                    log_event!(
                        "validator_removed",
                        near_sdk::serde_json::json!({
                            "vault": env::current_account_id(),
                            "validator": validator,
                        })
                    );
                }
            }
            Err(_) => {
                log_event!(
                    "staked_balance_callback_failed",
                    near_sdk::serde_json::json!({
                        "vault": env::current_account_id(),
                        "validator": validator,
                        "error": "Failed to retrieve staked balance"
                    })
                );
            }
        }
    }
}
