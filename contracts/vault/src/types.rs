use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::{AccountId, IntoStorageKey, NearToken};
use near_sdk::{EpochHeight, Gas};
use serde::Deserialize;

pub const GAS_FOR_WITHDRAW_ALL: Gas = Gas::from_tgas(20);
pub const GAS_FOR_VIEW_CALL: Gas = Gas::from_tgas(20);
pub const GAS_FOR_CALLBACK: Gas = Gas::from_tgas(20);
pub const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(20);
pub const GAS_FOR_DEPOSIT_AND_STAKE: Gas = Gas::from_tgas(200);
pub const GAS_FOR_UNSTAKE: Gas = Gas::from_tgas(200);
pub const STORAGE_BUFFER: u128 = 10_000_000_000_000_000_000_000;
pub const NUM_EPOCHS_TO_UNLOCK: EpochHeight = 4;
pub const MAX_COUNTER_OFFERS: u64 = 10;

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, serde::Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct UnstakeEntry {
    pub amount: u128,
    pub epoch_height: EpochHeight,
}

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

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct VaultViewState {
    pub owner: AccountId,
    pub index: u64,
    pub version: u64,
    pub pending_liquidity_request: Option<PendingLiquidityRequest>,
    pub liquidity_request: Option<LiquidityRequest>,
    pub accepted_offer: Option<AcceptedOffer>,
}

/// Describes a liquidity request pre-validation
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct PendingLiquidityRequest {
    pub token: AccountId,      // NEP-141 token used by lenders (e.g. USDC)
    pub amount: U128,          // Principal requested from lender
    pub interest: U128,        // Additional amount to be repaid
    pub collateral: NearToken, // NEAR collateral backing the loan
    pub duration: u64,         // Time in seconds before liquidation is allowed
}

/// Describes a liquidity request created by the vault owner
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

#[derive(serde::Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AcceptRequestMessage {
    /// "AcceptLiquidityRequest"
    pub action: String,
    pub token: AccountId,
    pub amount: U128,
    pub interest: U128,
    pub collateral: NearToken,
    pub duration: u64,
}

#[derive(serde::Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct CounterOfferMessage {
    /// NewCounterOffer
    pub action: String,
    pub token: AccountId,
    pub amount: U128,
    pub interest: U128,
    pub collateral: NearToken,
    pub duration: u64,
}

/// Represents the matched lender’s offer
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct AcceptedOffer {
    pub lender: AccountId,
    pub accepted_at: u64,
}

/// Tracks how much NEAR has been liquidated toward fulfilling the lender's claim
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct Liquidation {
    pub liquidated: NearToken,
}

/// Represents the derived lifecycle state of the vault
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub enum VaultState {
    Idle,
    Pending,
    Active,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct CounterOffer {
    pub proposer: AccountId,
    pub amount: U128,
    pub timestamp: u64,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct RefundEntry {
    pub token: AccountId,
    pub proposer: AccountId,
    pub amount: U128,
}
