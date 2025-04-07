#![allow(dead_code)]

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::{
    collections::{UnorderedSet, Vector},
    env, near_bindgen, AccountId, BorshStorageKey, PanicOnDefault,
};

#[derive(BorshDeserialize, BorshSerialize)]
pub struct UnstakeEntry {
    pub amount: u128,
    pub timestamp: u64,
}

#[derive(BorshStorageKey, BorshSerialize)]
pub enum StorageKey {
    ActiveValidators,
    UnbondingValidators,
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
    pub unbonding_validators: UnorderedSet<AccountId>,
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
            unbonding_validators: UnorderedSet::new(StorageKey::UnbondingValidators),
            unstake_entries: UnorderedMap::new(StorageKey::UnstakeEntries),
        }
    }
}
