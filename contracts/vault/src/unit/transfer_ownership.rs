#[path = "test_utils.rs"]
mod test_utils;

use crate::contract::Vault;
use near_sdk::{test_utils::get_logs, testing_env, NearToken};
use test_utils::{alice, get_context, owner};

#[test]
fn test_transfer_ownership_success() {
    // Set up context with the current owner and 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // New owner to transfer to
    let new_owner = alice();

    // Perform ownership transfer
    vault.transfer_ownership(new_owner.clone());

    // Assert that the owner was updated
    assert_eq!(
        vault.owner, new_owner,
        "Vault owner was not updated correctly"
    );

    // Check that the event was logged
    let logs = get_logs();
    let found = logs.iter().any(|log| log.contains("ownership_transferred"));
    assert!(
        found,
        "Expected 'ownership_transferred' log not found. Logs: {:?}",
        logs
    );
}

#[test]
#[should_panic(expected = "Only the vault owner can transfer ownership")]
fn test_transfer_ownership_rejects_non_owner() {
    // Set up context where alice (not the owner) is calling with 1 yoctoNEAR
    let context = get_context(
        alice(), // not the vault owner
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault with owner.near as the true owner
    let mut vault = Vault::new(owner(), 0, 1);

    // Alice attempts to transfer ownership to herself
    vault.transfer_ownership(alice());
}

#[test]
#[should_panic(expected = "New owner must be different from the current owner")]
fn test_transfer_ownership_rejects_same_owner() {
    // Set up context with the current owner and 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault with the owner
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to transfer ownership to self
    vault.transfer_ownership(owner());
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_transfer_ownership_requires_1yocto() {
    // Set up context with the correct owner, but no deposit
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        None, // No yoctoNEAR attached
    );
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Try to transfer ownership without attaching 1 yocto
    vault.transfer_ownership(alice());
}
