#![allow(dead_code)]

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::ext::ext_self;
use crate::ext::ext_staking_pool;
use crate::log_event;
use crate::types::{GAS_FOR_VIEW_CALL, GAS_FOR_WITHDRAW_ALL};
use near_sdk::{assert_one_yocto, env, near_bindgen, AccountId, Promise};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn claim_unstaked(&mut self, validator: AccountId) -> Promise {
        // Require 1 yoctoNEAR for access control
        assert_one_yocto();

        // Only the vault owner can perform this action
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the vault owner can claim unstaked balance"
        );

        // Trigger withdraw_all → then fetch updated unstaked balance
        ext_staking_pool::ext(validator.clone())
            .with_static_gas(GAS_FOR_WITHDRAW_ALL)
            .withdraw_all()
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_VIEW_CALL)
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
            env::panic_str("Failed to execute withdraw_all on validator");
        } else {
            log_event!(
                "claim_unstaked_completed",
                near_sdk::serde_json::json!({
                    "validator": validator,
                })
            );
            self.unstake_entries.remove(&validator);
        }
    }
}
