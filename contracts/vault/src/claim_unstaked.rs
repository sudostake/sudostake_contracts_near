use crate::contract::VaultExt;
use crate::ext_self;
use crate::log_event;
use crate::types::*;
use crate::Vault;
use near_sdk::json_types::U128;
use near_sdk::{assert_one_yocto, env, near_bindgen, AccountId, NearToken, Promise};

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

        // Emit event to track the flow
        log_event!(
            "claim_unstaked_started",
            near_sdk::serde_json::json!({ "validator": validator })
        );

        // Trigger withdraw_all → then fetch updated unstaked balance
        Promise::new(validator.clone())
            .function_call(
                "withdraw_all".to_string(),
                near_sdk::serde_json::json!({
                    "account_id": env::current_account_id()
                })
                .to_string()
                .into_bytes(),
                NearToken::from_yoctonear(0),
                GAS_FOR_WITHDRAW_ALL,
            )
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_VIEW_CALL)
                    .on_withdraw_all_returned_for_claim_unstaked(validator),
            )
    }

    #[private]
    pub fn on_withdraw_all_returned_for_claim_unstaked(&mut self, validator: AccountId) -> Promise {
        // Now query the validator for how much NEAR is still unclaimed (after withdraw_all)
        Promise::new(validator.clone())
            .function_call(
                "get_account_unstaked_balance".to_string(),
                near_sdk::serde_json::json!({
                    "account_id": env::current_account_id()
                })
                .to_string()
                .into_bytes(),
                NearToken::from_yoctonear(0),
                GAS_FOR_VIEW_CALL,
            )
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_account_unstaked_balance_returned_for_claim_unstaked(validator),
            )
    }

    #[private]
    pub fn on_account_unstaked_balance_returned_for_claim_unstaked(
        &mut self,
        validator: AccountId,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    ) {
        // Parse the returned balance or fail
        let remaining_unstaked = match result {
            Ok(value) => NearToken::from_yoctonear(value.0),
            Err(_) => env::panic_str("Failed to fetch unstaked balance from validator"),
        };

        // Compute how much was actually withdrawn
        let total_before = self.total_unstaked(&validator);
        let withdrawn = total_before
            .as_yoctonear()
            .saturating_sub(remaining_unstaked.as_yoctonear());

        // Reconcile internal records
        self.reconcile_unstake_entries(&validator, withdrawn);

        // Emit unstake_entries_reconciled event
        log_event!(
            "unstake_entries_reconciled",
            near_sdk::serde_json::json!({
                "validator": validator,
                "withdrawn": withdrawn.to_string(),
                "remaining": remaining_unstaked
            })
        );

        // Emit claim_unstaked_completed event
        log_event!(
            "claim_unstaked_completed",
            near_sdk::serde_json::json!({
                "validator": validator,
                "claimed": withdrawn.to_string()
            })
        );
    }
}
