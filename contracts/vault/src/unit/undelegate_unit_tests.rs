#[path = "test_utils.rs"]
mod test_utils;

use crate::{
    contract::Vault,
    types::{LiquidityRequest, UnstakeEntry},
};
use near_sdk::{env, json_types::U128, testing_env, AccountId, NearToken};
use test_utils::{alice, get_context, owner};

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_undelegate_fails_without_yocto() {
    // Set up context with NO attached deposit
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault with 10 NEAR
    let mut vault = Vault::new(owner(), 0, 1);

    // Add validator to active set
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Attempt to undelegate without 1 yoctoNEAR — should panic
    vault.undelegate(validator, NearToken::from_near(1));
}

#[test]
#[should_panic(expected = "Only the vault owner can undelegate")]
fn test_undelegate_fails_if_not_owner() {
    // Set up context where alice (not the owner) is calling with 1 yoctoNEAR
    let context = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Vault is owned by owner.near
    let mut vault = Vault::new(owner(), 0, 1);

    // Add validator to active set
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Alice attempts to undelegate — should panic
    vault.undelegate(validator, NearToken::from_near(1));
}

#[test]
#[should_panic(expected = "Amount must be greater than 0")]
fn test_undelegate_fails_if_zero_amount() {
    // Set up context as vault owner with 1 yoctoNEAR attached
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add validator to active set
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Attempt to undelegate 0 NEAR — should panic
    vault.undelegate(validator, NearToken::from_yoctonear(0));
}

#[test]
#[should_panic(expected = "Validator is not currently active")]
fn test_undelegate_fails_if_validator_not_active() {
    // Set up context as vault owner with 1 yoctoNEAR attached
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault with no active validators
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to undelegate from a validator not in the active set — should panic
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.undelegate(validator, NearToken::from_near(1));
}

#[test]
#[should_panic(expected = "Cannot undelegate when a liquidity request is open")]
fn test_undelegate_fails_if_liquidity_request_open() {
    // Set up context as vault owner with 1 yoctoNEAR attached
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add active validator
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Simulate a pending liquidity request
    let token: AccountId = "usdc.test.near".parse().unwrap();
    vault.liquidity_request = Some(LiquidityRequest {
        token: token.clone(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Attempt to undelegate — should panic
    vault.undelegate(validator, NearToken::from_near(1));
}

#[test]
#[should_panic(expected = "Processing undelegation already in progress")]
fn test_undelegate_fails_if_lock_active() {
    // Set up context as vault owner with 1 yoctoNEAR attached
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add validator to active set
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Attempt undelegation — works fine
    vault.undelegate(validator.clone(), NearToken::from_near(1));

    // Attempt undelegation again — should panic
    vault.undelegate(validator, NearToken::from_near(1));
}

#[test]
fn test_undelegate_succeeds_when_all_conditions_met() {
    // Set up context as vault owner with 1 yoctoNEAR attached
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add validator to active set
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Call undelegate — should succeed and return a Promise
    let result = vault.undelegate(validator, NearToken::from_near(1));
    assert!(matches!(result, near_sdk::Promise { .. }));
}

#[test]
fn test_on_unstake_complete_inserts_new_entry() {
    // Set up context as vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define validator and amount
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    let amount = NearToken::from_near(2);

    // Simulate successful unstake
    vault.on_unstake_complete(validator.clone(), amount, Ok(()));

    // Verify unstake entry was created
    let entry = vault
        .unstake_entries
        .get(&validator)
        .expect("Expected unstake entry to exist");
    assert_eq!(entry.amount, amount.as_yoctonear());
    assert_eq!(entry.epoch_height, env::epoch_height());
}

#[test]
fn test_on_unstake_complete_updates_existing_entry() {
    // Set up context as vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define validator and insert an existing entry with 1 NEAR
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    let initial_amount = NearToken::from_near(1).as_yoctonear();
    vault.unstake_entries.insert(
        &validator,
        &UnstakeEntry {
            amount: initial_amount,
            epoch_height: 100, // old epoch
        },
    );

    // Simulate an additional unstake of 2 NEAR
    let additional = NearToken::from_near(2);
    vault.on_unstake_complete(validator.clone(), additional, Ok(()));

    // Verify the entry was updated correctly
    let updated = vault
        .unstake_entries
        .get(&validator)
        .expect("Expected unstake entry to exist");
    assert_eq!(updated.amount, initial_amount + additional.as_yoctonear());
    assert_eq!(updated.epoch_height, env::epoch_height());
}

#[test]
fn test_on_unstake_complete_handles_failure_and_unlocks() {
    // Set up context as vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault with undelegation lock active
    let mut vault = Vault::new(owner(), 0, 1);
    vault.processing_undelegation = true;
    vault.processing_undelegation_since = env::block_timestamp();

    // Define validator and amount
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    let amount = NearToken::from_near(1);

    // Simulate a failed unstake callback
    vault.on_unstake_complete(validator, amount, Err(near_sdk::PromiseError::Failed));

    // Ensure the undelegation lock was cleared
    assert!(
        !vault.processing_undelegation,
        "Expected lock to be cleared"
    );
}

#[test]
fn test_on_account_staked_balance_removes_validator_if_balance_zero() {
    // Set up context as vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault with validator in active set
    let mut vault = Vault::new(owner(), 0, 1);
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Simulate callback with zero staked balance
    vault.on_account_staked_balance(validator.clone(), Ok(0.into()));

    // Validator should be removed from the active set
    assert!(
        !vault.active_validators.contains(&validator),
        "Validator should be removed if balance is zero"
    );
}

#[test]
fn test_on_account_staked_balance_keeps_validator_if_balance_nonzero() {
    // Set up context as vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault and add validator to active set
    let mut vault = Vault::new(owner(), 0, 1);
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Simulate callback with non-zero staked balance
    vault.on_account_staked_balance(validator.clone(), Ok(1_000_000.into()));

    // Validator should still be in the active set
    assert!(
        vault.active_validators.contains(&validator),
        "Validator should remain if balance is non-zero"
    );
}

#[test]
fn test_on_account_staked_balance_handles_failure_and_keeps_validator() {
    // Set up context as vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault with validator in active set
    let mut vault = Vault::new(owner(), 0, 1);
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Simulate callback failure
    vault.on_account_staked_balance(validator.clone(), Err(near_sdk::PromiseError::Failed));

    // Validator should still be in the active set
    assert!(
        vault.active_validators.contains(&validator),
        "Validator should remain if staked balance callback fails"
    );
}
