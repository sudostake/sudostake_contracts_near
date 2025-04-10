use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::IntoStorageKey;
use near_sdk::{EpochHeight, Gas};

pub const GAS_FOR_WITHDRAW_ALL: Gas = Gas::from_tgas(20);
pub const GAS_FOR_VIEW_CALL: Gas = Gas::from_tgas(20);
pub const GAS_FOR_CALLBACK: Gas = Gas::from_tgas(20);
pub const GAS_FOR_DEPOSIT_AND_STAKE: Gas = Gas::from_tgas(200);
pub const GAS_FOR_UNSTAKE: Gas = Gas::from_tgas(200);
pub const STORAGE_BUFFER: u128 = 10_000_000_000_000_000_000_000;

#[derive(BorshDeserialize, BorshSerialize)]
pub struct UnstakeEntry {
    pub amount: u128,
    pub epoch_height: EpochHeight,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub enum StorageKey {
    ActiveValidators,
    UnstakeEntries,
    UnstakeEntriesPerValidator { validator_hash: Vec<u8> },
}

impl IntoStorageKey for StorageKey {
    fn into_storage_key(self) -> Vec<u8> {
        near_sdk::borsh::to_vec(&self).expect("Failed to serialize storage key")
    }
}
