#[path = "test_utils.rs"]
mod test_utils;

use crate::{
    contract::Vault,
    types::{ProcessingState, UnstakeEntry},
};
use near_sdk::{env, test_utils::get_logs, testing_env, AccountId, NearToken};
use test_utils::{alice, get_context, get_context_with_timestamp, owner};

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_claim_unstaked_requires_yocto() {
    // Set up context with NO attached deposit
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to call claim_unstaked without attaching 1 yoctoNEAR
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.claim_unstaked(validator);
}

#[test]
#[should_panic(expected = "Only the vault owner can claim unstaked balance")]
fn test_claim_unstaked_rejects_non_owner() {
    // Set up context where alice (not the owner) is calling with 1 yoctoNEAR
    let context = get_context(
        alice(), // not the owner
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault with owner.near
    let mut vault = Vault::new(owner(), 0, 1);

    // Alice attempts to claim unstaked from the validator
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.claim_unstaked(validator);
}

#[test]
#[should_panic(expected = "No unstake entry found for validator")]
fn test_claim_unstaked_fails_if_no_entry() {
    // Set up context with the vault owner
    let context = get_context_with_timestamp(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
        None,
    );
    testing_env!(context);

    // Initialize vault with no unstake entries
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to claim unstaked without any entry
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.claim_unstaked(validator);
}

#[test]
#[should_panic(expected = "Unstaked funds not yet claimable")]
fn test_claim_unstaked_fails_if_epoch_not_ready() {
    // Set up context with vault owner and 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Add an unstake entry with recent epoch
    let current_epoch = env::epoch_height();
    vault.unstake_entries.insert(
        &validator,
        &UnstakeEntry {
            amount: NearToken::from_near(2).as_yoctonear(),
            epoch_height: current_epoch,
        },
    );

    // Attempt to claim unstaked early — should panic
    vault.claim_unstaked(validator);
}

#[test]
#[should_panic(expected = "Unstaked funds not yet claimable")]
fn test_claim_unstaked_resists_epoch_wraparound() {
    // Prepare context with 1 yoctoNEAR but low epoch height
    let mut context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    context.epoch_height = 10;
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Insert entry that would overflow without saturating add
    vault.unstake_entries.insert(
        &validator,
        &UnstakeEntry {
            amount: NearToken::from_near(1).as_yoctonear(),
            epoch_height: u64::MAX - 1,
        },
    );

    // Claim should still be blocked, verifying overflow fix
    vault.claim_unstaked(validator);
}

#[test]
#[should_panic(expected = "Cannot claim unstaked NEAR while liquidation is in progress")]
fn test_claim_unstaked_fails_if_liquidation_active() {
    // Set up a context with 1 yoctoNEAR and valid epoch
    let mut context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    context.epoch_height = 10;
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Insert a claimable unstake entry
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.unstake_entries.insert(
        &validator,
        &UnstakeEntry {
            amount: NearToken::from_near(2).as_yoctonear(),
            epoch_height: 5,
        },
    );

    // Simulate liquidation being active
    vault.liquidation = Some(crate::types::Liquidation {
        liquidated: NearToken::from_yoctonear(0),
    });

    // Attempt to claim while liquidation is active — should panic
    vault.claim_unstaked(validator);
}

#[test]
#[should_panic(expected = "Vault busy with ClaimUnstaked")]
fn test_claim_unstaked_fails_if_lock_active() {
    // Set up context with vault owner and 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // First acquisition succeeds
    vault.acquire_processing_lock(ProcessingState::ClaimUnstaked);

    // Second acquisition should panic because the lock is still held
    vault.acquire_processing_lock(ProcessingState::ClaimUnstaked);
}

#[test]
fn test_on_withdraw_all_handles_error_without_panic() {
    // Set up context with vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Insert dummy unstake entry
    vault.unstake_entries.insert(
        &validator,
        &UnstakeEntry {
            amount: NearToken::from_near(1).as_yoctonear(),
            epoch_height: env::epoch_height(),
        },
    );

    // Simulate a failed withdraw_all callback
    vault.on_withdraw_all(validator.clone(), Err(near_sdk::PromiseError::Failed));

    // Assert entry is NOT removed
    assert!(
        vault.unstake_entries.get(&validator).is_some(),
        "Entry should remain if withdraw_all fails"
    );

    // Verify claim_unstake_failed log event
    let logs = get_logs().join("");
    assert!(
        logs.contains("claim_unstake_failed"),
        "Log should contain 'claim_unstake_failed'"
    );

    // Check lock is released
    assert_eq!(
        vault.processing_state,
        ProcessingState::Idle,
        "Processing state should be reset to Idle"
    );
}

#[test]
fn test_on_withdraw_all_removes_entry() {
    // Set up context
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault with a dummy unstake entry
    let mut vault = Vault::new(owner(), 0, 1);
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.unstake_entries.insert(
        &validator,
        &UnstakeEntry {
            amount: 1_000_000_000_000_000_000_000_000,
            epoch_height: env::epoch_height(),
        },
    );

    // Simulate successful callback
    vault.on_withdraw_all(validator.clone(), Ok(()));

    // Assert entry is removed
    assert!(
        vault.unstake_entries.get(&validator).is_none(),
        "Expected unstake entry to be removed"
    );

    // Lock should be released
    assert_eq!(
        vault.processing_state,
        ProcessingState::Idle,
        "Processing state should be reset to Idle"
    );
}

#[test]
fn test_claim_unstaked_allows_after_epoch_passed() {
    // Set up a context with epoch_height high enough to unlock
    let mut context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    context.epoch_height = 10;
    testing_env!(context);

    // Initialize vault with an old enough unstake entry
    let mut vault = Vault::new(owner(), 0, 1);
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Entry becomes eligible when: epoch_height > entry.epoch_height + 4
    // So entry.epoch_height = 10 - 5 = 5
    vault.unstake_entries.insert(
        &validator,
        &UnstakeEntry {
            amount: NearToken::from_near(2).as_yoctonear(),
            epoch_height: 5,
        },
    );

    // Call claim_unstaked — should not panic
    let _ = vault.claim_unstaked(validator);
}
