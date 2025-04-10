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
    pub fn delegate(&mut self, validator: AccountId, amount: NearToken) -> Promise {
        assert_one_yocto();

        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the vault owner can delegate stake"
        );

        assert!(amount.as_yoctonear() > 0, "Amount must be greater than 0");

        let available_balance = self.get_available_balance();
        assert!(
            amount.as_yoctonear() <= available_balance.as_yoctonear(),
            "Requested amount ({}) exceeds vault balance ({})",
            amount.as_yoctonear(),
            available_balance
        );

        let has_pending_unstakes = self
            .unstake_entries
            .get(&validator)
            .map(|q| !q.is_empty())
            .unwrap_or(false);

        if !has_pending_unstakes {
            log_event!(
                "delegate_direct",
                near_sdk::serde_json::json!({
                    "validator": validator.clone(),
                    "amount": amount
                })
            );

            return Promise::new(validator.clone())
                .function_call(
                    "deposit_and_stake".to_string(),
                    vec![],
                    amount,
                    GAS_FOR_DEPOSIT_AND_STAKE,
                )
                .then(
                    ext_self::ext(env::current_account_id())
                        .with_static_gas(GAS_FOR_CALLBACK)
                        .on_deposit_and_stake_returned_for_delegate(validator, amount),
                );
        }

        log_event!(
            "delegate_started",
            near_sdk::serde_json::json!({
                "validator": validator,
                "amount": amount
            })
        );

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
                    .on_withdraw_all_returned_for_delegate(validator, amount),
            )
    }

    #[private]
    pub fn on_withdraw_all_returned_for_delegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
    ) -> Promise {
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
                    .on_account_unstaked_balance_returned_for_delegate(validator, amount),
            )
    }

    #[private]
    pub fn on_account_unstaked_balance_returned_for_delegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    ) -> Promise {
        let remaining_unstaked = match result {
            Ok(value) => NearToken::from_yoctonear(value.0),
            Err(_) => env::panic_str("Failed to fetch unstaked balance from validator"),
        };

        let total_before = self.total_unstaked(&validator);
        let withdrawn = total_before
            .as_yoctonear()
            .saturating_sub(remaining_unstaked.as_yoctonear());

        self.reconcile_unstake_entries(&validator, withdrawn);

        log_event!(
            "unstake_entries_reconciled",
            near_sdk::serde_json::json!({
                "validator": validator,
                "withdrawn": withdrawn.to_string(),
                "remaining": remaining_unstaked,
            })
        );

        Promise::new(validator.clone())
            .function_call(
                "deposit_and_stake".to_string(),
                vec![],
                amount,
                GAS_FOR_DEPOSIT_AND_STAKE,
            )
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_deposit_and_stake_returned_for_delegate(validator, amount),
            )
    }

    #[private]
    pub fn on_deposit_and_stake_returned_for_delegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) {
        if result.is_err() {
            env::panic_str("Failed to execute deposit_and_stake on validator");
        }

        self.active_validators.insert(&validator);

        log_event!(
            "validator_activated",
            near_sdk::serde_json::json!({ "validator": validator })
        );

        log_event!(
            "delegate_completed",
            near_sdk::serde_json::json!({
                "validator": validator,
                "amount": amount
            })
        );
    }
}
