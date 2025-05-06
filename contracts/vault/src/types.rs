//! This module defines all staking-related constants and types
//! used across the Vault contract logic, including lifecycle tracking,
//! validator management, and refund handling.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::{AccountId, IntoStorageKey, NearToken};
use near_sdk::{EpochHeight, Gas};
use serde::Deserialize;

// === Gas Constants ===

/// Gas allocated for withdrawing unstaked balance from a validator.
pub const GAS_FOR_WITHDRAW_ALL: Gas = Gas::from_tgas(35);
/// Gas allocated for calling view methods on external contracts.
pub const GAS_FOR_VIEW_CALL: Gas = Gas::from_tgas(10);
/// Gas allocated for internal callback resolution logic.
pub const GAS_FOR_CALLBACK: Gas = Gas::from_tgas(20);
/// Gas allocated for transferring FT tokens.
pub const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(30);
/// Gas allocated for depositing and staking NEAR on a validator.
pub const GAS_FOR_DEPOSIT_AND_STAKE: Gas = Gas::from_tgas(120);
/// Gas allocated for unstaking NEAR from a validator.
pub const GAS_FOR_UNSTAKE: Gas = Gas::from_tgas(60);

// === Protocol Constants ===

/// Amount of NEAR reserved in the vault to prevent deletion from storage exhaustion.
pub const STORAGE_BUFFER: u128 = 10_000_000_000_000_000_000_000; // 0.01 NEAR
/// Number of epochs required before unstaked NEAR becomes withdrawable.
pub const NUM_EPOCHS_TO_UNLOCK: EpochHeight = 4;
/// Epochs after which failed refunds expire.
pub const REFUND_EXPIRY_EPOCHS: EpochHeight = 4;
/// Maximum number of active counter offers stored per vault.
pub const MAX_COUNTER_OFFERS: u64 = 7;
/// Maximum number of validators a vault can actively stake with.
pub const MAX_ACTIVE_VALIDATORS: u64 = 2;
/// Time in nanoseconds before a processing lock becomes stale.
pub const LOCK_TIMEOUT: u64 = 30 * 60 * 1_000_000_000; // 30 minutes

// === Enums ===

/// Indicates which long-running operation is currently locked in the vault.
#[derive(BorshSerialize, BorshDeserialize, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum ProcessingState {
    /// No operation is currently in progress.
    Idle = 0,
    /// Vault is currently processing an undelegation.
    Undelegate = 1,
    /// Vault is currently processing a loan repayment.
    RepayLoan = 2,
    /// Vault is currently processing lender claims during liquidation.
    ProcessClaims = 3,
    /// Vault is currently processing request liquidity
    RequestLiquidity = 4,
}

/// Tracks how much NEAR is unstaked and the epoch when it will be available.
#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, serde::Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct UnstakeEntry {
    pub amount: u128,
    pub epoch_height: EpochHeight,
}

/// Keys used to index stored collections in contract storage.
#[derive(BorshSerialize, BorshDeserialize)]
pub enum StorageKey {
    ActiveValidators,
    CounterOffers,
    UnstakeEntries,
    RefundList,
}

impl IntoStorageKey for StorageKey {
    fn into_storage_key(self) -> Vec<u8> {
        near_sdk::borsh::to_vec(&self).expect("Failed to serialize storage key")
    }
}

/// Public view state returned from the `get_vault_state` method.
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct VaultViewState {
    pub owner: AccountId,
    pub index: u64,
    pub version: u64,
    pub pending_liquidity_request: Option<PendingLiquidityRequest>,
    pub liquidity_request: Option<LiquidityRequest>,
    pub accepted_offer: Option<AcceptedOffer>,
    pub is_listed_for_takeover: bool,
}

/// Liquidity request under construction — not yet validated or accepted.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct PendingLiquidityRequest {
    pub token: AccountId,
    pub amount: U128,
    pub interest: U128,
    pub collateral: NearToken,
    pub duration: u64,
}

/// A finalized liquidity request created by the vault owner.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct LiquidityRequest {
    pub token: AccountId,
    pub amount: U128,
    pub interest: U128,
    pub collateral: NearToken,
    pub duration: u64,
    pub created_at: u64,
}

/// Message format used to accept a liquidity request.
#[derive(serde::Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AcceptRequestMessage {
    pub action: String,
    pub token: AccountId,
    pub amount: U128,
    pub interest: U128,
    pub collateral: NearToken,
    pub duration: u64,
}

/// Message format used by lenders to propose a counter offer.
#[derive(serde::Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct CounterOfferMessage {
    pub action: String,
    pub token: AccountId,
    pub amount: U128,
    pub interest: U128,
    pub collateral: NearToken,
    pub duration: u64,
}

/// Matched offer from a lender, recorded after acceptance.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct AcceptedOffer {
    pub lender: AccountId,
    pub accepted_at: u64,
}

/// Tracks the cumulative amount liquidated in NEAR to fulfill a lender's claim.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct Liquidation {
    pub liquidated: NearToken,
}

/// Logical vault status based on its lending lifecycle.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub enum VaultState {
    Idle,
    Pending,
    Active,
}

/// A counter offer submitted by a lender with proposed terms.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct CounterOffer {
    pub proposer: AccountId,
    pub amount: U128,
    pub timestamp: u64,
}

/// Refund entry representing a failed `ft_transfer` refund.
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
pub struct RefundEntry {
    pub token: Option<AccountId>,
    pub proposer: AccountId,
    pub amount: U128,
    pub added_at_epoch: EpochHeight,
}
