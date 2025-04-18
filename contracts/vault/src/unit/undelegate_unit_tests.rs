#[path = "test_utils.rs"]
mod test_utils;

use crate::{contract::Vault, types::UnstakeEntry};
use near_sdk::{env, testing_env, AccountId, NearToken};
use test_utils::{alice, get_context, owner};

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_undelegate_requires_yocto() {
    // Set up context with NO attached deposit
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Register the validator as active
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Attempt to call undelegate with no attached deposit
    // This should panic due to assert_one_yocto()
    vault.undelegate(validator, NearToken::from_near(1));
}

#[test]
#[should_panic(expected = "Only the vault owner can undelegate")]
fn test_undelegate_requires_owner() {
    // Context: alice is NOT the vault owner
    let context = get_context(
        alice(), // <-- caller is alice
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Vault is owned by owner.near
    let mut vault = Vault::new(owner(), 0, 1);

    // Register the validator as active
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Alice tries to undelegate — should panic
    vault.undelegate(validator, NearToken::from_near(1));
}

#[test]
#[should_panic(expected = "Amount must be greater than 0")]
fn test_undelegate_rejects_zero_amount() {
    // Set up context with correct owner and 1 yoctoNEAR deposit
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Register validator as active
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Attempt to undelegate 0 NEAR — should panic
    vault.undelegate(validator, NearToken::from_yoctonear(0));
}

#[test]
#[should_panic(expected = "Validator is not currently active")]
fn test_undelegate_requires_active_validator() {
    // Set up context with owner and valid deposit
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault with owner
    let mut vault = Vault::new(owner(), 0, 1);

    // Use a validator that hasn't been added to active_validators
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Attempt to undelegate — should panic due to missing validator
    vault.undelegate(validator, NearToken::from_near(1));
}

#[test]
#[should_panic(expected = "Cannot undelegate after a liquidity request has been accepted")]
fn test_undelegate_fails_if_offer_accepted() {
    // Set up context as vault owner with 1 yocto
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault with accepted offer
    let mut vault = Vault::new(owner(), 0, 1);
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    vault.active_validators.insert(&validator);
    vault.accepted_offer = Some(crate::types::AcceptedOffer {
        lender: "lender.near".parse().unwrap(),
        accepted_at: 12345678,
    });

    // Try undelegating — should panic
    vault.undelegate(validator, NearToken::from_near(1));
}

#[test]
fn test_on_unstake_inserts_entry() {
    // Set up context with the vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define validator and amount
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    let amount = NearToken::from_near(2);

    // Simulate successful on_unstake callback
    vault.on_unstake(validator.clone(), amount, Ok(()));

    // Assert that an entry was created in unstake_entries
    let entry = vault
        .unstake_entries
        .get(&validator)
        .expect("Expected unstake entry to be inserted");
    assert_eq!(entry.amount, amount.as_yoctonear());
    assert_eq!(entry.epoch_height, env::epoch_height());
}

#[test]
fn test_on_unstake_updates_existing_entry() {
    // Set up context with the vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define a validator and pre-insert an unstake entry
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    let initial_amount = NearToken::from_near(1).as_yoctonear();
    vault.unstake_entries.insert(
        &validator,
        &UnstakeEntry {
            amount: initial_amount,
            epoch_height: 100,
        },
    );

    // Simulate a second successful unstake of 2 NEAR
    let additional = NearToken::from_near(2);
    vault.on_unstake(validator.clone(), additional, Ok(()));

    // Assert that the entry was updated correctly
    let entry = vault
        .unstake_entries
        .get(&validator)
        .expect("Expected unstake entry to be present");
    assert_eq!(entry.amount, initial_amount + additional.as_yoctonear());
    assert_eq!(entry.epoch_height, env::epoch_height());
}

#[test]
#[should_panic(expected = "Failed to execute unstake on validator")]
fn test_on_unstake_panics_on_failure() {
    // Set up context with the vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define validator and amount
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    let amount = NearToken::from_near(1);

    // Simulate a failed callback result
    vault.on_unstake(validator, amount, Err(near_sdk::PromiseError::Failed));
}
