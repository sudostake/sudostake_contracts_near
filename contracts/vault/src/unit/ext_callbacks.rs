#[path = "test_utils.rs"]
mod test_utils;

use crate::{contract::Vault, types::STORAGE_BUFFER};
use near_sdk::{testing_env, AccountId, NearToken};
use test_utils::{get_context, owner};

#[test]
#[should_panic(expected = "Failed to execute deposit_and_stake on validator")]
fn test_on_delegate_complete_panics_on_failure() {
    // Set up test context with the vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define a dummy validator
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Simulate a failed deposit_and_stake callback
    vault.on_deposit_and_stake(
        validator,
        NearToken::from_near(1),
        Err(near_sdk::PromiseError::Failed),
    );
}

#[test]
fn test_get_available_balance_subtracts_storage_buffer() {
    // Total account balance set to 1 NEAR
    let context = get_context(owner(), NearToken::from_near(1), None);
    testing_env!(context);

    // Initialize vault
    let vault = Vault::new(owner(), 0, 1);

    // Expected available balance: 1 NEAR - STORAGE_BUFFER
    let expected = 1_000_000_000_000_000_000_000_000u128 - STORAGE_BUFFER;

    assert_eq!(
        vault.get_available_balance().as_yoctonear(),
        expected,
        "get_available_balance() should subtract STORAGE_BUFFER correctly"
    );
}
