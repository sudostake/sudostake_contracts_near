//! Message payloads exchanged via `ft_transfer_call`.

use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{AccountId, NearToken};

#[cfg(not(target_arch = "wasm32"))]
use schemars::JsonSchema;

/// Action literal expected in `ApplyCounterOfferMessage`.
pub const APPLY_COUNTER_OFFER_ACTION: &str = "ApplyCounterOffer";

/// Message format used by lenders to apply their funds toward a request.
#[derive(Deserialize, Serialize, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(JsonSchema))]
#[serde(crate = "near_sdk::serde")]
pub struct ApplyCounterOfferMessage {
    /// Expected action literal ("ApplyCounterOffer").
    pub action: String,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Token contract offered by the lender.
    pub token: AccountId,
    #[cfg_attr(not(target_arch = "wasm32"), schemars(with = "String"))]
    /// Principal amount recorded against the request. This must always equal the
    /// original `LiquidityRequest.amount`; lenders propose smaller counter offers
    /// solely by attaching a lower `ft_on_transfer` deposit than the value here.
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
