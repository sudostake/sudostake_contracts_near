use crate::contract::VaultExt;
use crate::ext_self;
use crate::log_event;
use crate::types::*;
use crate::Vault;
use crate::METHOD_GET_ACCOUNT_STAKED_BALANCE;
use crate::METHOD_GET_ACCOUNT_UNSTAKED_BALANCE;
use crate::METHOD_UNSTAKE;
use crate::METHOD_WITHDRAW_ALL;
use near_sdk::collections::Vector;
use near_sdk::json_types::U128;
use near_sdk::{assert_one_yocto, env, near_bindgen, AccountId, NearToken, Promise};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn undelegate(&mut self, validator: AccountId, amount: NearToken) -> Promise {
        // Require 1 yoctoNEAR for access control
        assert_one_yocto();

        // Only the vault owner can undelegate
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the vault owner can undelegate"
        );

        // Amount must be greater than 0
        assert!(amount.as_yoctonear() > 0, "Amount must be greater than 0");

        // Validator must be currently active
        assert!(
            self.active_validators.contains(&validator),
            "Validator is not currently active"
        );

        // Emit undelegate_started event
        log_event!(
            "undelegate_started",
            near_sdk::serde_json::json!({
                "validator": validator,
                "amount": amount,
            })
        );

        // Query the validator for the current staked balance
        Promise::new(validator.clone())
            .function_call(
                METHOD_GET_ACCOUNT_STAKED_BALANCE.to_string(),
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
                    .on_account_staked_balance_returned_for_undelegate(validator, amount),
            )
    }

    #[private]
    pub fn on_account_staked_balance_returned_for_undelegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    ) -> Promise {
        // Inspect amount of gas left
        self.log_gas_checkpoint("on_account_staked_balance_returned_for_undelegate");

        let staked_balance = match result {
            Ok(value) => NearToken::from_yoctonear(value.0),
            Err(_) => env::panic_str("Failed to fetch staked balance from validator"),
        };

        // Check that the validator has enough stake to undelegate the requested amount
        assert!(
            staked_balance >= amount,
            "Not enough staked balance to undelegate. Requested: {}, Available: {}",
            amount.as_yoctonear(),
            staked_balance.as_yoctonear()
        );

        // We should remove this validator from the active validators
        // When the user is unstaking all their funds
        let should_remove_validator = staked_balance == amount;

        // Emit undelegate_check_passed event
        log_event!(
            "undelegate_check_passed",
            near_sdk::serde_json::json!({
                "validator": validator,
                "staked_balance": staked_balance,
                "requested": amount
            })
        );

        // Call withdraw_all to pull any pending unstaked funds before proceeding
        Promise::new(validator.clone())
            .function_call(
                METHOD_WITHDRAW_ALL.to_string(),
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
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_withdraw_all_returned_for_undelegate(
                        validator,
                        amount,
                        should_remove_validator,
                    ),
            )
    }

    #[private]
    pub fn on_withdraw_all_returned_for_undelegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        should_remove_validator: bool,
    ) -> Promise {
        // Inspect amount of gas left
        self.log_gas_checkpoint("on_withdraw_all_returned_for_undelegate");

        // Call get_account_unstaked_balance to determine how much remains unwithdrawn
        Promise::new(validator.clone())
            .function_call(
                METHOD_GET_ACCOUNT_UNSTAKED_BALANCE.to_string(),
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
                    .on_account_unstaked_balance_returned_for_undelegate(
                        validator,
                        amount,
                        should_remove_validator,
                    ),
            )
    }

    #[private]
    pub fn on_account_unstaked_balance_returned_for_undelegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        should_remove_validator: bool,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    ) -> Promise {
        // Inspect amount of gas left
        self.log_gas_checkpoint("on_account_unstaked_balance_returned_for_undelegate");

        // Parse the returned unstaked balance after withdraw_all
        let remaining_unstaked = match result {
            Ok(value) => NearToken::from_yoctonear(value.0),
            Err(_) => env::panic_str("Failed to fetch unstaked balance from validator"),
        };

        // Sync unstake entries after withdraw to match staking_pool
        self.reconcile_after_withdraw(&validator, remaining_unstaked);

        // Emit log to confirm unstake action initiated
        log_event!(
            "unstake_initiated",
            near_sdk::serde_json::json!({
                "validator": validator,
                "amount": amount,
            })
        );

        // Prepare unstake arguments for the staking_pool contract
        let json_args = near_sdk::serde_json::to_vec(&near_sdk::serde_json::json!({
            "amount": amount.as_yoctonear().to_string()
        }))
        .unwrap();

        // Proceed with unstaking the intended amount
        Promise::new(validator.clone())
            .function_call(
                METHOD_UNSTAKE.to_string(),
                json_args,
                NearToken::from_yoctonear(0),
                GAS_FOR_UNSTAKE,
            )
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_unstake_returned_for_undelegate(validator, amount, should_remove_validator),
            )
    }

    #[private]
    pub fn on_unstake_returned_for_undelegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        should_remove_validator: bool,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) {
        // Inspect amount of gas left
        self.log_gas_checkpoint("on_unstake_returned_for_undelegate");

        // Ensure the unstake call succeeded
        if result.is_err() {
            env::panic_str("Failed to execute unstake on validator");
        }

        // Remove validator from the active list is remaining stake is 0
        if should_remove_validator {
            self.active_validators.remove(&validator);
            log_event!(
                "validator_removed",
                near_sdk::serde_json::json!({ "validator": validator })
            );
        }

        // Construct the new unstake entry using current epoch height
        let entry = UnstakeEntry {
            amount: amount.as_yoctonear(),
            epoch_height: env::epoch_height(),
        };

        // Get or create the unstake entry queue for the validator
        let mut queue = self.unstake_entries.get(&validator).unwrap_or_else(|| {
            Vector::new(StorageKey::UnstakeEntriesPerValidator {
                validator_hash: env::sha256(validator.as_bytes()),
            })
        });

        // Add the new entry to the validator's queue
        queue.push(&entry);

        // Persist the updated queue to state
        self.unstake_entries.insert(&validator, &queue);

        // Emit undelegate_completed event
        log_event!(
            "undelegate_completed",
            near_sdk::serde_json::json!({
                "validator": validator,
                "amount": amount
            })
        );
    }
}
