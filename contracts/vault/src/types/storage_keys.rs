//! Storage key definitions for persistent collections.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::IntoStorageKey;

/// Keys used to index stored collections in contract storage.
#[derive(BorshSerialize, BorshDeserialize)]
pub enum StorageKey {
    ActiveValidators,
    CounterOffers,
    UnstakeEntries,
    RefundList,
}

impl IntoStorageKey for StorageKey {
    fn into_storage_key(self) -> Vec<u8> {
        borsh::to_vec(&self).expect("Failed to serialize storage key")
    }
}
