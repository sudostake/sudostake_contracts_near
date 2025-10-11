//! Message payloads exchanged via `ft_transfer_call`.

use near_sdk::json_types::U128;
use near_sdk::serde::Deserialize;
use near_sdk::{AccountId, NearToken};

#[cfg(not(target_arch = "wasm32"))]
use schemars::JsonSchema;

/// Message format used to accept a liquidity request.
#[derive(Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct AcceptRequestMessage {
    /// Expected action literal ("AcceptLiquidityRequest").
    pub action: String,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Token contract accepted for repayment.
    pub token: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Principal amount provided by the lender.
    pub amount: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Interest amount the lender will receive.
    pub interest: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Collateral amount the borrower will lock.
    pub collateral: NearToken,
    /// Loan duration in seconds.
    pub duration: u64,
}

/// Message format used by lenders to propose a counter offer.
#[derive(Deserialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct CounterOfferMessage {
    /// Expected action literal ("NewCounterOffer").
    pub action: String,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Token contract offered by the lender.
    pub token: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Principal amount the lender will provide.
    pub amount: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Interest the lender requests.
    pub interest: U128,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Collateral amount proposed by the lender.
    pub collateral: NearToken,
    /// Proposed loan duration in seconds.
    pub duration: u64,
}
