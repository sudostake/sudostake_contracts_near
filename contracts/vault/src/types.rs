//! Shared types and constants used across the Vault contract.
//! The module centralises staking constants, storage keys, business data
//! structures, and their serde/ABI representations.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{AccountId, IntoStorageKey, NearToken};
use near_sdk::{EpochHeight, Gas};

#[cfg(not(target_arch = "wasm32"))]
use schemars::JsonSchema;

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
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum ProcessingState {
    /// No operation is currently in progress.
    Idle = 0,
    /// Vault is currently processing delegation.
    Delegate = 1,
    /// Vault is currently processing claiming unstaked balance from validator.
    ClaimUnstaked = 2,
    /// Vault is currently processing request liquidity.
    RequestLiquidity = 3,
    /// Vault is currently processing an undelegation.
    Undelegate = 4,
    /// Vault is currently processing a loan repayment.
    RepayLoan = 5,
    /// Vault is currently processing lender claims during liquidation.
    ProcessClaims = 6,
}
/// Public view state returned from the `get_vault_state` method.
#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct VaultViewState {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub owner: AccountId,
    pub index: u64,
    pub version: u64,
    pub liquidity_request: Option<LiquidityRequest>,
    pub accepted_offer: Option<AcceptedOffer>,
    pub is_listed_for_takeover: bool,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "Vec<String>"))]
    pub active_validators: Vec<AccountId>,
    #[cfg_attr(
        not(target_arch = "wasm32"),
        schemars(with = "Vec<(String, UnstakeEntry)>")
    )]
    pub unstake_entries: Vec<(AccountId, UnstakeEntry)>,
    pub liquidation: Option<Liquidation>,
    pub current_epoch: EpochHeight,
}

/// Tracks how much NEAR is unstaked and the epoch when it will be available.
#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct UnstakeEntry {
    pub amount: u128,
    pub epoch_height: EpochHeight,
}

// === Storage Keys ===

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
        borsh::to_vec(&self).expect("Failed to serialize storage key")
    }
}

// === Liquidity Request Types ===

/// Liquidity request under construction — not yet validated or accepted.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct PendingLiquidityRequest {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub token: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub amount: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub interest: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub collateral: NearToken,
    pub duration: u64,
}

/// A finalized liquidity request created by the vault owner.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct LiquidityRequest {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub token: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub amount: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub interest: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub collateral: NearToken,
    pub duration: u64,
    pub created_at: u64,
}

/// Message format used to accept a liquidity request.
#[derive(Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct AcceptRequestMessage {
    pub action: String,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub token: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub amount: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub interest: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub collateral: NearToken,
    pub duration: u64,
}

/// Message format used by lenders to propose a counter offer.
#[derive(Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct CounterOfferMessage {
    pub action: String,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub token: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub amount: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub interest: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub collateral: NearToken,
    pub duration: u64,
}

/// Matched offer from a lender, recorded after acceptance.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct AcceptedOffer {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub lender: AccountId,
    pub accepted_at: u64,
}

/// A counter offer submitted by a lender with proposed terms.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct CounterOffer {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub proposer: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub amount: U128,
    pub timestamp: u64,
}

/// Tracks the cumulative amount liquidated in NEAR to fulfill a lender's claim.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct Liquidation {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub liquidated: NearToken,
}

/// Refund entry representing a failed `ft_transfer` refund.
#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct RefundEntry {
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "Option<String>"))]
    pub token: Option<AccountId>,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub proposer: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    pub amount: U128,
    pub added_at_epoch: EpochHeight,
}
