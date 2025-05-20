#![allow(dead_code)]

use crate::contract::{Vault, VaultExt};
use crate::types::{CounterOffer, RefundEntry, UnstakeEntry, VaultViewState};
use near_sdk::json_types::U128;
use near_sdk::{env, near_bindgen, AccountId};

#[near_bindgen]
impl Vault {
    /// Returns a snapshot of the current vault state,
    /// including ownership, loan status, validators, and current epoch.
    pub fn get_vault_state(&self) -> VaultViewState {
        VaultViewState {
            owner: self.owner.clone(),
            index: self.index,
            version: self.version,
            liquidity_request: self.liquidity_request.clone(),
            accepted_offer: self.accepted_offer.clone(),
            is_listed_for_takeover: self.is_listed_for_takeover,
            active_validators: self.active_validators.to_vec(),
            unstake_entries: self.unstake_entries.iter().collect(),
            liquidation: self.liquidation.clone(),
            current_epoch: env::epoch_height(),
        }
    }

    /// Returns a list of active validator account IDs this vault is currently staked with.
    pub fn get_active_validators(&self) -> Vec<String> {
        self.active_validators
            .to_vec()
            .into_iter()
            .map(|a| a.to_string())
            .collect()
    }

    /// Returns the unstake entry for a specific validator, if it exists.
    pub fn get_unstake_entry(&self, validator: AccountId) -> Option<UnstakeEntry> {
        self.unstake_entries.get(&validator)
    }

    /// Returns the list of active counter offers if any exist for the current liquidity request.
    pub fn get_counter_offers(&self) -> Option<std::collections::HashMap<AccountId, CounterOffer>> {
        self.counter_offers
            .as_ref()
            .map(|map| map.to_vec().into_iter().collect())
    }

    /// Returns the available NEAR balance in the vault that can be withdrawn or delegated.
    pub fn view_available_balance(&self) -> U128 {
        self.get_available_balance().as_yoctonear().into()
    }

    /// Returns the current storage cost (in yoctoNEAR) required to keep the vault alive.
    pub fn view_storage_cost(&self) -> U128 {
        U128(self.get_storage_cost())
    }

    /// Returns the full list of refund entries, or filters by account ID if provided.
    ///
    /// # Arguments
    ///
    /// * `account_id` - Optional NEAR account ID to filter refunds by proposer.
    ///
    /// # Returns
    ///
    /// A vector of (index, RefundEntry) tuples.
    pub fn get_refund_entries(&self, account_id: Option<AccountId>) -> Vec<(u64, RefundEntry)> {
        match account_id {
            Some(target) => self
                .refund_list
                .iter()
                .filter(|(_, entry)| entry.proposer == target)
                .collect(),
            None => self.refund_list.iter().collect(),
        }
    }
}
