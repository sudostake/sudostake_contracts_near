use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::{
    env, near_bindgen, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault, Promise,
};
use serde::{Deserialize, Serialize};

pub const UNSTAKE_DELAY_EPOCHS: u64 = 4;

#[derive(BorshSerialize, BorshStorageKey)]
pub enum StorageKey {
    Accounts,
}

/// Simulates the staking pool's inner account state.
#[derive(BorshDeserialize, BorshSerialize, Default)]
pub struct Account {
    /// Unstaked funds available after a delay.
    pub unstaked: u128,
    /// Number of stake shares owned (1:1 for mock purposes).
    pub stake_shares: u128,
    /// Epoch height when the unstaked amount becomes withdrawable.
    pub unstaked_available_epoch_height: u64,
}

/// Mirrors the real staking pool's public read structure.
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct HumanReadableAccount {
    pub account_id: AccountId,
    pub unstaked_balance: U128,
    pub staked_balance: U128,
    pub can_withdraw: bool,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct MockStakingPool {
    accounts: LookupMap<AccountId, Account>,
    total_staked_balance: u128,
    last_total_balance: u128,
}

#[near_bindgen]
impl MockStakingPool {
    #[init]
    pub fn new() -> Self {
        Self {
            accounts: LookupMap::new(StorageKey::Accounts),
            total_staked_balance: 0,
            last_total_balance: 0,
        }
    }

    #[payable]
    pub fn deposit_and_stake(&mut self) -> Promise {
        let account_id = env::predecessor_account_id();
        let deposit = env::attached_deposit().as_yoctonear();
        assert!(deposit > 0, "Must attach a deposit");

        let mut account = self.accounts.get(&account_id).unwrap_or_default();

        account.stake_shares += deposit;
        self.total_staked_balance += deposit;

        self.last_total_balance = env::account_balance().as_yoctonear() + self.total_staked_balance;

        self.accounts.insert(&account_id, &account);

        Promise::new(env::current_account_id()).function_call(
            "noop".to_string(),
            vec![],
            NearToken::from_yoctonear(0),
            Gas::from_tgas(5),
        )
    }

    #[payable]
    pub fn unstake(&mut self, amount: U128) -> Promise {
        let account_id = env::predecessor_account_id();
        let mut account = self.accounts.get(&account_id).unwrap_or_default();
        let amount = amount.0;

        assert!(account.stake_shares >= amount, "Not enough staked balance");

        // Set the withdraw timer only if this is the first unstake in the round
        if account.unstaked == 0 {
            account.unstaked_available_epoch_height = env::epoch_height() + UNSTAKE_DELAY_EPOCHS;
        }

        account.stake_shares -= amount;
        account.unstaked += amount;
        self.total_staked_balance -= amount;

        self.accounts.insert(&account_id, &account);

        env::log_str(&format!(
            "@{} unstaked {} yocto (available at epoch {})",
            account_id, amount, account.unstaked_available_epoch_height
        ));

        Promise::new(env::current_account_id()).function_call(
            "noop".to_string(),
            vec![],
            NearToken::from_yoctonear(0),
            Gas::from_tgas(5),
        )
    }

    pub fn withdraw_all(&mut self) -> Promise {
        let account_id = env::predecessor_account_id();
        let current_epoch = env::epoch_height();
        let mut account = self.accounts.get(&account_id).unwrap_or_default();

        assert!(account.unstaked > 0, "No unstaked balance");

        assert!(
            current_epoch >= account.unstaked_available_epoch_height,
            "Unstaking not yet matured. Current epoch: {}, required: {}",
            current_epoch,
            account.unstaked_available_epoch_height
        );

        let amount = account.unstaked;
        account.unstaked = 0;
        account.unstaked_available_epoch_height = 0;

        self.accounts.insert(&account_id, &account);

        env::log_str(&format!(
            "@{} withdrawing {} yocto at epoch {}",
            account_id, amount, current_epoch
        ));

        Promise::new(account_id.clone()).transfer(NearToken::from_yoctonear(amount))
    }

    pub fn get_account_unstaked_balance(&self, account_id: AccountId) -> U128 {
        let account = self.accounts.get(&account_id).unwrap_or_default();
        U128(account.unstaked)
    }

    pub fn get_account_staked_balance(&self, account_id: AccountId) -> U128 {
        let account = self.accounts.get(&account_id).unwrap_or_default();
        U128(account.stake_shares)
    }

    pub fn get_account(&self, account_id: AccountId) -> HumanReadableAccount {
        let account = self.accounts.get(&account_id).unwrap_or_default();
        let current_epoch = env::epoch_height();

        let can_withdraw =
            account.unstaked > 0 && current_epoch >= account.unstaked_available_epoch_height;

        HumanReadableAccount {
            account_id,
            unstaked_balance: U128(account.unstaked),
            staked_balance: U128(account.stake_shares),
            can_withdraw,
        }
    }

    #[private]
    pub fn noop(&self) {
        // Does nothing. Placeholder to complete the promise.
    }
}
