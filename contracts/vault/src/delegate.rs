#![allow(dead_code)]

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::ext::ext_self;
use crate::ext::ext_staking_pool;
use crate::log_event;
use crate::types::{GAS_FOR_CALLBACK, GAS_FOR_DEPOSIT_AND_STAKE};
use near_sdk::{assert_one_yocto, env, near_bindgen, AccountId, NearToken, Promise};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn delegate(&mut self, validator: AccountId, amount: NearToken) -> Promise {
        // Ensure the call is intentional
        assert_one_yocto();

        // Ensure only the vault owner can delegate
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the vault owner can delegate stake"
        );

        // Ensure amount is greater than 0
        assert!(amount.as_yoctonear() > 0, "Amount must be greater than 0");

        // Ensure there is enough balance to delegate
        let available_balance = self.get_available_balance();
        assert!(
            amount.as_yoctonear() <= available_balance.as_yoctonear(),
            "Requested amount ({}) exceeds vault balance ({})",
            amount.as_yoctonear(),
            available_balance
        );

        // Call deposit and stake on ext_staking_pool
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

        if result.is_err() {
            env::panic_str("Failed to execute deposit_and_stake on validator");
        } else {
            self.active_validators.insert(&validator);

            log_event!(
                "delegate_completed",
                near_sdk::serde_json::json!({
                    "validator": validator,
                    "amount": amount
                })
            );
        }
    }
}
