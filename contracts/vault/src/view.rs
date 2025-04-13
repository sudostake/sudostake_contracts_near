#![allow(dead_code)]

use crate::contract::{Vault, VaultExt};
use crate::types::{UnstakeEntry, VaultViewState};
use near_sdk::{near_bindgen, AccountId};

#[near_bindgen]
impl Vault {
    pub fn get_unstake_entries(&self, validator: AccountId) -> Vec<UnstakeEntry> {
        self.unstake_entries
            .get(&validator)
            .map(|q| q.to_vec())
            .unwrap_or_default()
    }

    pub fn get_vault_state(&self) -> VaultViewState {
        VaultViewState {
            owner: self.owner.clone(),
            index: self.index,
            version: self.version,
        }
    }
}
