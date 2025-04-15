#![allow(dead_code)]

use crate::contract::{Vault, VaultExt};
use crate::types::VaultViewState;
use near_sdk::near_bindgen;

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
}
