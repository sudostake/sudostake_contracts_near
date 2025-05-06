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
/// Represents the complete on-chain state of a single SudoStake vault instance.
/// A vault allows its owner to stake NEAR, request liquidity against it, and manage repayments or liquidation flows.
pub struct Vault {
    /// Account ID of the vault owner (the borrower/staker).
    pub owner: AccountId,
    /// Vault index assigned by the factory at creation (used to generate subaccount name).
    pub index: u64,
    /// Version of the vault contract code at the time of deployment.
    pub version: u64,
    /// Set of validators currently holding delegated stake from this vault.
    pub active_validators: UnorderedSet<AccountId>,
    /// Tracks unstaked NEAR and the epoch in which each unstake was requested, grouped per validator.
    pub unstake_entries: UnorderedMap<AccountId, UnstakeEntry>,
    /// Temporary placeholder for a liquidity request before validator balances are verified.
    pub pending_liquidity_request: Option<PendingLiquidityRequest>,
    /// Current active liquidity request posted by the vault owner. Only one may exist at a time.
    pub liquidity_request: Option<LiquidityRequest>,
    /// Active counter offers submitted by lenders in response to the open liquidity request.
    /// This map is cleared when an offer is accepted or the request is canceled.
    pub counter_offers: Option<UnorderedMap<AccountId, CounterOffer>>,
    /// Details of the lender whose offer was accepted, marking the loan as active and enforceable.
    pub accepted_offer: Option<AcceptedOffer>,
    /// Tracks how much NEAR has been liquidated so far, once the loan defaults past its expiry.
    pub liquidation: Option<Liquidation>,
    /// Stores refund entries that failed (e.g., due to `ft_transfer` rejections),
    /// allowing them to be retried later by the vault owner or proposer.
    pub refund_list: UnorderedMap<u64, RefundEntry>,
    /// Monotonically increasing counter used as a key for refund entries, ensuring deterministic order.
    pub refund_nonce: u64,
    /// Current long-running operation in progress (e.g., repay, claim, or undelegate).
    pub processing_state: ProcessingState,
    /// Block timestamp of when the current processing state began. Used to detect stale locks.
    pub processing_since: u64,
    /// Flag indicating whether this vault is listed for public takeover.
    /// If true, anyone can buy this vault by paying the listed takeover fee.
    pub is_listed_for_takeover: bool,
}

#[near_bindgen]
impl Vault {
    /// Initializes a new vault instance with the specified owner, index, and version.
    /// This method is callable only once per vault contract (via `#[init]`).
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
    /// Test-only method: overrides the timestamp of the accepted offer.
    /// This is used to simulate time-based behaviors such as loan expiry and liquidation.
    pub fn set_accepted_offer_timestamp(&mut self, timestamp: u64) {
        if let Some(offer) = &mut self.accepted_offer {
            offer.accepted_at = timestamp;
        } else {
            env::panic_str("No accepted offer found");
        }
    }
}
