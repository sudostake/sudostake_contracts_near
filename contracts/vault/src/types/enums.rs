//! Enum types shared across the contract.

use near_sdk::borsh::{BorshDeserialize, BorshSerialize};

#[cfg(not(target_arch = "wasm32"))]
use schemars::JsonSchema;

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
    /// Vault takeover (claim_vault) is currently in flight.
    ClaimVault = 7,
}
