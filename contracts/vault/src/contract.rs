#![allow(dead_code)]

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::{assert_one_yocto, EpochHeight, Gas, NearToken, Promise};
use near_sdk::{
    collections::{UnorderedSet, Vector},
    env, near_bindgen, AccountId, BorshStorageKey, PanicOnDefault,
};

const GAS_FOR_WITHDRAW_ALL: Gas = Gas::from_tgas(20);
const GAS_FOR_VIEW_CALL: Gas = Gas::from_tgas(20);
const GAS_FOR_CALLBACK: Gas = Gas::from_tgas(20);
const GAS_FOR_DEPOSIT_AND_STAKE: Gas = Gas::from_tgas(200);
const GAS_FOR_UNSTAKE: Gas = Gas::from_tgas(200);
/// 0.1 NEAR
pub const STORAGE_BUFFER: u128 = 10_000_000_000_000_000_000_000;

#[derive(BorshDeserialize, BorshSerialize)]
pub struct UnstakeEntry {
    pub amount: u128,
    pub epoch_height: EpochHeight,
}

#[derive(BorshStorageKey, BorshSerialize)]
pub enum StorageKey {
    ActiveValidators,
    UnstakeEntries,
    UnstakeEntryPerValidator { validator_hash: Vec<u8> },
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Vault {
    pub owner: AccountId,
    pub index: u64,
    pub version: u64,
    pub active_validators: UnorderedSet<AccountId>,
    pub unstake_entries: UnorderedMap<AccountId, Vector<UnstakeEntry>>,
}

#[near_bindgen]
impl Vault {
    #[init]
    pub fn new(owner: AccountId, index: u64, version: u64) -> Self {
        assert!(!env::state_exists(), "Contract already initialized");

        log_event!(
            "vault_created",
            near_sdk::serde_json::json!({
                "owner": owner,
                "index": index,
                "version": version
            })
        );

        Self {
            owner,
            index,
            version,
            active_validators: UnorderedSet::new(StorageKey::ActiveValidators),
            unstake_entries: UnorderedMap::new(StorageKey::UnstakeEntries),
        }
    }
}

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn delegate(&mut self, validator: AccountId, amount: NearToken) -> Promise {
        // Ensure the function is called intentionally with exactly 1 yoctoNEAR
        assert_one_yocto();

        // Only the vault owner is allowed to call this method
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the vault owner can delegate stake"
        );

        // The amount to delegate must be greater than zero
        assert!(amount.as_yoctonear() > 0, "Amount must be greater than 0");

        // Ensure the vault has enough available balance to cover the delegation
        let available_balance = self.get_available_balance();
        assert!(
            amount.as_yoctonear() <= available_balance.as_yoctonear(),
            "Requested amount ({}) exceeds vault balance ({})",
            amount.as_yoctonear(),
            available_balance
        );

        // Track this validator as active
        self.active_validators.insert(&validator);

        // Optimization: If there are no pending unstake entries, skip withdrawal and reconciliation
        let has_pending_unstakes = self
            .unstake_entries
            .get(&validator)
            .map(|q| !q.is_empty())
            .unwrap_or(false);

        if !has_pending_unstakes {
            log_event!(
                "delegate_direct",
                near_sdk::serde_json::json!({
                    "validator": validator,
                    "amount": amount
                })
            );

            return Promise::new(validator).function_call(
                "deposit_and_stake".to_string(),
                vec![],
                amount,
                GAS_FOR_DEPOSIT_AND_STAKE,
            );
        }

        // Standard path: begin delegation with withdraw_all followed by reconciliation and staking
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
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_VIEW_CALL)
                    .on_withdraw_and_delegate(validator, amount),
            )
    }

    #[private]
    pub fn on_withdraw_and_delegate(&mut self, validator: AccountId, amount: NearToken) -> Promise {
        // Once withdraw_all resolves, fetch how much is still pending unbonded
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
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_reconciled_delegate(validator, amount),
            )
    }

    #[private]
    pub fn on_reconciled_delegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    ) -> Promise {
        // Parse the returned unstaked balance after withdraw_all
        let remaining_unstaked = match result {
            Ok(value) => NearToken::from_yoctonear(value.0),
            Err(_) => env::panic_str("Failed to fetch unstaked balance from validator"),
        };

        // Determine how much was withdrawn by comparing with previous total
        let total_before = self.total_unstaked(&validator);
        let withdrawn = total_before
            .as_yoctonear()
            .saturating_sub(remaining_unstaked.as_yoctonear());

        // Update unstake_entries based on withdrawn amount
        self.reconcile_unstake_entries(&validator, withdrawn);

        log_event!(
            "unstake_entries_reconciled",
            near_sdk::serde_json::json!({
                "validator": validator,
                "withdrawn": withdrawn,
                "remaining": remaining_unstaked,
            })
        );

        log_event!(
            "delegate_completed",
            near_sdk::serde_json::json!({
                "validator": validator,
                "amount": amount
            })
        );

        // Proceed with staking the intended amount
        Promise::new(validator).function_call(
            "deposit_and_stake".to_string(),
            vec![],
            amount,
            GAS_FOR_DEPOSIT_AND_STAKE,
        )
    }
}

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
                "get_account_staked_balance".to_string(),
                near_sdk::serde_json::json!({
                    "account_id": env::current_account_id()
                })
                .to_string()
                .into_bytes(),
                NearToken::from_yoctonear(0),
                GAS_FOR_VIEW_CALL,
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_checked_staked_balance(validator, amount),
            )
    }

    #[private]
    pub fn on_checked_staked_balance(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    ) -> Promise {
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
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_withdraw_and_unstake(validator, amount),
            )
    }

    #[private]
    pub fn on_withdraw_and_unstake(&mut self, validator: AccountId, amount: NearToken) -> Promise {
        // Call get_account_unstaked_balance to determine how much remains unwithdrawn
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
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_reconciled_unstake(validator, amount),
            )
    }

    #[private]
    pub fn on_reconciled_unstake(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    ) -> Promise {
        // Parse the returned unstaked balance after withdraw_all
        let remaining_unstaked = match result {
            Ok(value) => NearToken::from_yoctonear(value.0),
            Err(_) => env::panic_str("Failed to fetch unstaked balance from validator"),
        };

        // Determine how much was withdrawn by comparing with previous total
        let total_before = self.total_unstaked(&validator);
        let withdrawn = total_before
            .as_yoctonear()
            .saturating_sub(remaining_unstaked.as_yoctonear());

        // Update unstake_entries based on withdrawn amount
        self.reconcile_unstake_entries(&validator, withdrawn);

        // Emit log to confirm unstake entries are reconciled
        log_event!(
            "unstake_entries_reconciled",
            near_sdk::serde_json::json!({
                "validator": validator,
                "withdrawn": withdrawn,
                "remaining": remaining_unstaked,
            })
        );

        // Emit log to confirm unstake action initiated
        log_event!(
            "unstake_initiated",
            near_sdk::serde_json::json!({
                "validator": validator,
                "amount": amount,
            })
        );

        // Proceed with unstaking the intended amount
        Promise::new(validator.clone())
            .function_call(
                "unstake".to_string(),
                near_sdk::serde_json::json!({
                    "amount": amount
                })
                .to_string()
                .into_bytes(),
                NearToken::from_yoctonear(0),
                GAS_FOR_UNSTAKE,
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_unstake_complete(validator, amount),
            )
    }

    #[private]
    pub fn on_unstake_complete(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) {
        // Ensure the unstake call succeeded
        if result.is_err() {
            env::panic_str("Failed to execute unstake on validator");
        }

        // Construct the new unstake entry using current epoch height
        let entry = UnstakeEntry {
            amount: amount.as_yoctonear(),
            epoch_height: env::epoch_height(),
        };

        // Get or create the unstake entry queue for the validator
        let mut queue = self.unstake_entries.get(&validator).unwrap_or_else(|| {
            Vector::new(StorageKey::UnstakeEntryPerValidator {
                validator_hash: env::sha256(validator.as_bytes()),
            })
        });

        // Add the new entry to the validator's queue
        queue.push(&entry);

        // Persist the updated queue to state
        self.unstake_entries.insert(&validator, &queue);

        // Emit a log to confirm the entry was added
        log_event!(
            "unstake_entry_added",
            near_sdk::serde_json::json!({
                "validator": validator,
                "amount": amount,
                "epoch_height": entry.epoch_height
            })
        );
    }
}

impl Vault {
    // Sums all pending unstaked amounts for a validator
    fn total_unstaked(&self, validator: &AccountId) -> NearToken {
        self.unstake_entries
            .get(validator)
            .map(|queue| queue.iter().map(|entry| entry.amount).sum::<u128>())
            .map(NearToken::from_yoctonear)
            .unwrap_or_else(|| NearToken::from_yoctonear(0))
    }

    // Removes unstake entries that were claimed via withdraw_all
    pub fn reconcile_unstake_entries(&mut self, validator: &AccountId, mut withdrawn: u128) {
        if let Some(queue) = self.unstake_entries.get(validator) {
            let mut new_queue = Vector::new(StorageKey::UnstakeEntryPerValidator {
                validator_hash: env::sha256(validator.as_bytes()),
            });

            for entry in queue.iter() {
                if withdrawn >= entry.amount {
                    withdrawn = withdrawn.saturating_sub(entry.amount);
                } else {
                    new_queue.push(&entry);
                }
            }

            if new_queue.is_empty() {
                self.unstake_entries.remove(validator);
            } else {
                self.unstake_entries.insert(validator, &new_queue);
            }
        }
    }

    // Returns the available balance after subtracting a fixed storage buffer
    pub fn get_available_balance(&self) -> NearToken {
        let total = env::account_balance().as_yoctonear();
        let available = total.saturating_sub(STORAGE_BUFFER);
        NearToken::from_yoctonear(available)
    }
}
