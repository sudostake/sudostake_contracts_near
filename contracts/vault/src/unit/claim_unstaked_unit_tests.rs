#[path = "test_utils.rs"]
mod test_utils;

use crate::{contract::Vault, types::UnstakeEntry};
use near_sdk::{env, testing_env, AccountId, NearToken};
use test_utils::{alice, get_context, owner};

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
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
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
#[should_panic(expected = "Failed to execute withdraw_all on validator")]
fn test_on_withdraw_all_panics_on_error() {
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
    vault.on_withdraw_all(validator, Err(near_sdk::PromiseError::Failed));
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
