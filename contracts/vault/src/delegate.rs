#![allow(dead_code)]

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::ext::ext_self;
use crate::ext::ext_staking_pool;
use crate::log_event;
use crate::types::ProcessingState;
use crate::types::MAX_ACTIVE_VALIDATORS;
use crate::types::{GAS_FOR_CALLBACK, GAS_FOR_DEPOSIT_AND_STAKE};
use near_sdk::{assert_one_yocto, env, near_bindgen, require, AccountId, NearToken, Promise};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn delegate(&mut self, validator: AccountId, amount: NearToken) -> Promise {
        // Require 1 yoctoNEAR for intentional call
        assert_one_yocto();

        // Only the vault owner can delegate
        require!(
            env::predecessor_account_id() == self.owner,
            "Only the vault owner can delegate stake"
        );

        // Limit to MAX_ACTIVE_VALIDATORS
        // If validator is new, enforce the active‑set size limit.
        if !self.active_validators.contains(&validator) {
            require!(
                self.active_validators.len() < MAX_ACTIVE_VALIDATORS,
                format!(
                    "You can only stake with {:?} validators at a time",
                    MAX_ACTIVE_VALIDATORS
                ),
            );
        }

        // Amount must be greater than 0
        require!(amount.as_yoctonear() > 0, "Amount must be greater than 0");

        // Ensure enough liquid balance to delegate
        let available = self.get_available_balance();
        require!(
            amount <= available,
            format!(
                "Requested amount ({}) exceeds available balance ({})",
                amount.as_yoctonear(),
                available.as_yoctonear()
            )
        );

        // Prevent delegation when refund_list is not empty
        require!(
            self.refund_list.is_empty(),
            "Cannot delegate while there are pending refund entries"
        );

        // Prevent delegation when liquidation is active
        require!(
            self.liquidation.is_none(),
            "Cannot delegate while liquidation is in progress"
        );

        // Lock the vault for **Delegate** workflow
        self.acquire_processing_lock(ProcessingState::Delegate);

        // Initiate deposit_and_stake on validator
        ext_staking_pool::ext(validator.clone())
            .with_static_gas(GAS_FOR_DEPOSIT_AND_STAKE)
            .with_attached_deposit(amount)
            .deposit_and_stake()
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_deposit_and_stake(validator, amount),
            )
    }

    #[private]
    pub fn on_deposit_and_stake(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) {
        // Inspect amount of gas left
        self.log_gas_checkpoint("on_deposit_and_stake");
        self.release_processing_lock();

        // The attached deposit will be refunded automatically by NEAR runtime
        if result.is_err() {
            log_event!(
                "delegate_failed",
                near_sdk::serde_json::json!({
                    "vault": env::current_account_id(),
                    "validator": validator,
                    "amount": amount,
                    "error": "deposit_and_stake failed"
                })
            );

            return;
        }

        // Add validator to active set
        self.active_validators.insert(&validator);

        // Log delegate_completed event
        log_event!(
            "delegate_completed",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "validator": validator,
                "amount": amount
            })
        );
    }
}
