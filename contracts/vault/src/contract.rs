#![allow(dead_code)]

use crate::log_event;
use crate::types::{
    AcceptedOffer, CounterOffer, Liquidation, LiquidityRequest, PendingLiquidityRequest,
    ProcessingState, RefundEntry, StorageKey, UnstakeEntry,
};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::{collections::UnorderedSet, env, near_bindgen, AccountId, PanicOnDefault};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
/// Represents the complete state of a SudoStake vault instance.
pub struct Vault {
    /// The account ID of the vault owner (borrower/staker).
    pub owner: AccountId,
    /// The vault's index as assigned by the factory at creation.
    pub index: u64,
    /// Version of the vault code used during deployment.
    pub version: u64,
    /// Set of currently active validators this vault has delegated to.
    pub active_validators: UnorderedSet<AccountId>,
    /// Tracks unstaked NEAR amounts and the epoch in which they were requested,
    /// grouped by validator.
    pub unstake_entries: UnorderedMap<AccountId, UnstakeEntry>,
    /// Temporarily stores a liquidity request while validator stake is being verified.
    pub pending_liquidity_request: Option<PendingLiquidityRequest>,
    /// Active liquidity request created by the vault owner.
    /// This is what lenders respond to with counter offers.
    pub liquidity_request: Option<LiquidityRequest>,
    /// Map of active counter offers from different lenders.
    /// Only exists while a liquidity request is open and no offer has been accepted.
    pub counter_offers: Option<UnorderedMap<AccountId, CounterOffer>>,
    /// The lender whose counter offer was accepted by the vault owner.
    /// This marks the loan as active and enforceable.
    pub accepted_offer: Option<AcceptedOffer>,
    /// Tracks how much collateral has been liquidated after loan expiration.
    /// This is initialized when liquidation begins.
    pub liquidation: Option<Liquidation>,
    /// Stores refunds that failed (e.g., due to ft_transfer failures),
    /// allowing them to be retried later.
    pub refund_list: UnorderedMap<u64, RefundEntry>,
    /// Unique incrementing nonce for refund entries to ensure consistent ordering.
    pub refund_nonce: u64,
    /// True while the vault is processing a request
    pub processing_state: ProcessingState,
    /// Prevents deadlock when processing_state is not Idle
    pub processing_since: u64,
    /// Indicates this vault can be taken over by anyone that pays the
    /// storage cost to the current owner
    pub is_listed_for_takeover: bool,
}

#[near_bindgen]
impl Vault {
    #[init]
    pub fn new(owner: AccountId, index: u64, version: u64) -> Self {
        assert!(!env::state_exists(), "Contract already initialized");

        log_event!(
            "vault_created",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
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
            refund_list: UnorderedMap::new(StorageKey::RefundList),
            refund_nonce: 0,
            processing_state: ProcessingState::Idle,
            processing_since: 0,
            is_listed_for_takeover: false,
        }
    }
}

#[cfg(feature = "integration-test")]
#[near_bindgen]
impl Vault {
    /// Test-only method: overrides accepted_offer timestamp for liquidation tests
    pub fn set_accepted_offer_timestamp(&mut self, timestamp: u64) {
        if let Some(offer) = &mut self.accepted_offer {
            offer.accepted_at = timestamp;
        } else {
            env::panic_str("No accepted offer found");
        }
    }
}
