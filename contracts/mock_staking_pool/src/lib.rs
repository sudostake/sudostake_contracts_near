// mock_staking_pool.rs
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, near_bindgen, AccountId, PanicOnDefault, BorshStorageKey, NearToken, Promise, Gas};
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use serde::Serialize;

#[derive(BorshSerialize, BorshStorageKey)]
pub enum StorageKey {
    Accounts,
}

#[derive(BorshDeserialize, BorshSerialize, Default)]
pub struct Account {
    pub staked_balance: u128,
    pub unstaked_balance: u128,
    pub unstake_epoch: u64,
    pub reward: u128,
    pub last_updated_epoch: u64,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AccountView {
    pub account_id: AccountId,
    pub staked_balance: String,
    pub unstaked_balance: String,
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
        account.staked_balance += deposit;
        account.last_updated_epoch = env::epoch_height();
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
        assert!(account.staked_balance >= amount, "Not enough staked balance");
        account.staked_balance -= amount;
        account.unstaked_balance += amount;
        account.unstake_epoch = env::epoch_height();
        account.last_updated_epoch = env::epoch_height();
        self.total_staked_balance -= amount;
        self.accounts.insert(&account_id, &account);
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
        assert!(account.unstaked_balance > 0, "No unstaked balance");
        assert!(current_epoch > account.unstake_epoch, "Unstaking not yet matured");
        let amount = account.unstaked_balance;
        account.unstaked_balance = 0;
        self.accounts.insert(&account_id, &account);
        env::log_str(&format!("@{} withdrawing {} yocto", account_id, amount));
        Promise::new(account_id.clone()).transfer(NearToken::from_yoctonear(amount))
    }

    pub fn get_account_unstaked_balance(&self, account_id: AccountId) -> U128 {
        let account = self.accounts.get(&account_id).unwrap_or_default();
        U128(account.unstaked_balance)
    }

    pub fn get_account_staked_balance(&self, account_id: AccountId) -> U128 {
        let account = self.accounts.get(&account_id).unwrap_or_default();
        U128(account.staked_balance)
    }

    pub fn get_account(&self, account_id: AccountId) -> AccountView {
        let account = self.accounts.get(&account_id).unwrap_or_default();
        let current_epoch = env::epoch_height();
        let can_withdraw = account.unstaked_balance > 0 && current_epoch > account.unstake_epoch;
        AccountView {
            account_id,
            staked_balance: account.staked_balance.to_string(),
            unstaked_balance: account.unstaked_balance.to_string(),
            can_withdraw,
        }
    }

    #[private]
    pub fn noop(&self) {
        // Does nothing. Placeholder to complete the promise.
    }
}