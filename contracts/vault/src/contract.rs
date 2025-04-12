#![allow(dead_code)]

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::{
    collections::{UnorderedSet, Vector},
    env, near_bindgen, AccountId, PanicOnDefault,
};

use crate::log_event;
use crate::types::*;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Vault {
    pub owner: AccountId,
    pub index: u64,
    pub version: u64,
    pub active_validators: UnorderedSet<AccountId>,
    pub unstake_entries: UnorderedMap<AccountId, Vector<UnstakeEntry>>,
    pub pending_liquidity_request: Option<PendingLiquidityRequest>,
    pub liquidity_request: Option<LiquidityRequest>,
    pub counter_offers: Option<UnorderedMap<AccountId, CounterOffer>>,
    pub accepted_offer: Option<AcceptedOffer>,
    pub liquidation: Option<Liquidation>,
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
            unstake_entries: UnorderedMap::new(StorageKey::UnstakeEntries),
            pending_liquidity_request: None,
            liquidity_request: None,
            counter_offers: None,
            accepted_offer: None,
            liquidation: None,
        }
    }
}
