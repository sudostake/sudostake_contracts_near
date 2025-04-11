use crate::Vault;
use crate::{contract::VaultExt, UnstakeEntry};
use near_sdk::{near_bindgen, AccountId};

#[near_bindgen]
impl Vault {
    pub fn get_unstake_entries(&self, validator: AccountId) -> Vec<UnstakeEntry> {
        self.unstake_entries
            .get(&validator)
            .map(|q| q.to_vec())
            .unwrap_or_default()
    }
}
