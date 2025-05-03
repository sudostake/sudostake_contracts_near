#![allow(dead_code)]

use crate::contract::{Vault, VaultExt};
use crate::types::{CounterOffer, RefundEntry, UnstakeEntry, VaultViewState};
use near_sdk::json_types::U128;
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
            is_listed_for_takeover: self.is_listed_for_takeover,
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

    pub fn view_available_balance(&self) -> U128 {
        self.get_available_balance().as_yoctonear().into()
    }

    pub fn view_storage_cost(&self) -> U128 {
        U128(self.get_storage_cost())
    }

    pub fn get_all_refund_entries(&self) -> Vec<(u64, RefundEntry)> {
        self.refund_list.iter().collect()
    }
}
