//! Gas and protocol-wide constants.

use near_sdk::{EpochHeight, Gas};

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

/// Number of yoctoNEAR in one NEAR.
pub const YOCTO_PER_NEAR: u128 = 10u128.pow(24);
/// Amount of NEAR reserved in the vault to prevent deletion from storage exhaustion.
pub const STORAGE_BUFFER: u128 = YOCTO_PER_NEAR / 100; // 0.01 NEAR
/// Number of epochs required before unstaked NEAR becomes withdrawable.
pub const NUM_EPOCHS_TO_UNLOCK: EpochHeight = 4;
/// Epochs after which failed refunds expire.
pub const REFUND_EXPIRY_EPOCHS: EpochHeight = 4;
/// Maximum number of active counter offers stored per vault.
pub const MAX_COUNTER_OFFERS: u64 = 7;
/// Maximum number of validators a vault can actively stake with.
pub const MAX_ACTIVE_VALIDATORS: u64 = 2;
/// Number of nanoseconds in one second.
pub const NANOS_PER_SECOND: u64 = 1_000_000_000;
/// Time in nanoseconds before a processing lock becomes stale.
pub const LOCK_TIMEOUT: u64 = 30 * 60 * NANOS_PER_SECOND; // 30 minutes
/// Maximum supported loan duration in seconds.
/// Computed so that `duration * 1e9` stays within `u64::MAX` for timestamp math.
pub const MAX_LOAN_DURATION: u64 = u64::MAX / 1_000_000_000 - 1;
