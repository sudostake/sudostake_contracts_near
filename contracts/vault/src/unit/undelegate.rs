#[path = "test_utils.rs"]
mod test_utils;

use crate::contract::Vault;
use near_sdk::{testing_env, AccountId, NearToken};
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
