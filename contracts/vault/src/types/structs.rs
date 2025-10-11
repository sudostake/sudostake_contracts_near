//! Core data structures persisted or exposed by the vault.

use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{AccountId, EpochHeight, NearToken};

#[cfg(not(target_arch = "wasm32"))]
use schemars::JsonSchema;

/// Tracks how much NEAR is unstaked and the epoch when it will be available.
#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct UnstakeEntry {
    /// Amount of NEAR (yocto) awaiting withdrawal.
    pub amount: u128,
    /// Epoch height when the funds become claimable.
    pub epoch_height: EpochHeight,
}

/// Liquidity request under construction — not yet validated or accepted.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct PendingLiquidityRequest {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Token contract expected from the lender.
    pub token: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Principal amount requested (token units).
    pub amount: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Interest the borrower agrees to pay.
    pub interest: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// NEAR collateral locked for the loan.
    pub collateral: NearToken,
    /// Requested loan duration in seconds.
    pub duration: u64,
}

/// A finalized liquidity request created by the vault owner.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct LiquidityRequest {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Token contract accepted for repayment.
    pub token: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Principal amount requested.
    pub amount: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Interest to be repaid on top of principal.
    pub interest: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Collateral the borrower committed in NEAR.
    pub collateral: NearToken,
    /// Loan duration in seconds.
    pub duration: u64,
    /// Timestamp (ns) when the request was created.
    pub created_at: u64,
}

/// Matched offer from a lender, recorded after acceptance.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct AcceptedOffer {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Account ID of the accepted lender.
    pub lender: AccountId,
    /// Timestamp (ns) when the offer was accepted.
    pub accepted_at: u64,
}

/// A counter offer submitted by a lender with proposed terms.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct CounterOffer {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Lender proposing the counter offer.
    pub proposer: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Amount offered by the lender.
    pub amount: U128,
    /// Block timestamp when the offer was submitted.
    pub timestamp: u64,
}

/// Tracks the cumulative amount liquidated in NEAR to fulfill a lender's claim.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct Liquidation {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Total amount of NEAR liquidated so far.
    pub liquidated: NearToken,
}

/// Refund entry representing a failed `ft_transfer` refund.
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct RefundEntry {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "Option<String>"))]
    /// Optional token address to refund (None = NEAR).
    pub token: Option<AccountId>,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Account awaiting the refund.
    pub proposer: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Amount to refund.
    pub amount: U128,
    /// Epoch when this refund entry was recorded.
    pub added_at_epoch: EpochHeight,
}

/// Public view state returned from the `get_vault_state` method.
#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct VaultViewState {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Account ID of the vault owner.
    pub owner: AccountId,
    /// Vault index assigned by the factory.
    pub index: u64,
    /// Contract code version deployed to this vault.
    pub version: u64,
    /// Current liquidity request, if any.
    pub liquidity_request: Option<LiquidityRequest>,
    /// Accepted lender offer, if a loan is active.
    pub accepted_offer: Option<AcceptedOffer>,
    /// Whether the vault is listed for takeover.
    pub is_listed_for_takeover: bool,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "Vec<String>"))]
    /// Delegated validators.
    pub active_validators: Vec<AccountId>,
    #[cfg_attr(
        not(target_arch = "wasm32"),
        schemars(with = "Vec<(String, UnstakeEntry)>")
    )]
    /// Unstake entries grouped by validator.
    pub unstake_entries: Vec<(AccountId, UnstakeEntry)>,
    /// Liquidation progress data, if applicable.
    pub liquidation: Option<Liquidation>,
    /// Block epoch height when this view was produced.
    pub current_epoch: EpochHeight,
}
