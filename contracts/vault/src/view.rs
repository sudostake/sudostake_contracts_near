#![allow(dead_code)]

use crate::contract::{Vault, VaultExt};
use crate::types::{CounterOffer, UnstakeEntry, VaultViewState};
use near_sdk::{near_bindgen, AccountId};

#[near_bindgen]
impl Vault {
    pub fn get_vault_state(&self) -> VaultViewState {
        VaultViewState {
            owner: self.owner.clone(),
            index: self.index,
            version: self.version,
            pending_liquidity_request: self.pending_liquidity_request.clone(),
            liquidity_request: self.liquidity_request.clone(),
            accepted_offer: self.accepted_offer.clone(),
        }
    }

    pub fn get_active_validators(&self) -> Vec<String> {
        self.active_validators
            .to_vec()
            .into_iter()
            .map(|a| a.to_string())
            .collect()
    }

    pub fn get_unstake_entry(&self, validator: AccountId) -> Option<UnstakeEntry> {
        self.unstake_entries.get(&validator)
    }

    pub fn get_counter_offers(&self) -> Option<std::collections::HashMap<AccountId, CounterOffer>> {
        self.counter_offers
            .as_ref()
            .map(|map| map.to_vec().into_iter().collect())
    }
}
