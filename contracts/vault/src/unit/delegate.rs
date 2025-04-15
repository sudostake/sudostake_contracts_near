#[path = "test_utils.rs"]
mod test_utils;
use crate::contract::Vault;
use near_sdk::{testing_env, NearToken};
use test_utils::{alice, get_context, owner};

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_delegate_fails_if_no_attached_deposit() {
    // Simulate a context where owner.near is calling the contract with 10 NEAR
    // and no attached deposit
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize a vault owned by owner.near
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to call delegate method
    // This should panic due to assert_one_yocto
    vault.delegate(
        "validator.poolv1.near".parse().unwrap(),
        NearToken::from_yoctonear(1),
    );
}

#[test]
#[should_panic(expected = "Only the vault owner can delegate stake")]
fn test_delegate_fails_if_not_owner() {
    // alice tries to call delegate on a vault owned by owner.near
    let context = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault with owner.near as the owner
    let mut vault = Vault::new(owner(), 0, 1);

    // alice (not the owner) attempts to call delegate
    vault.delegate(
        "validator.poolv1.near".parse().unwrap(),
        NearToken::from_yoctonear(1),
    );
}

#[test]
#[should_panic(expected = "Amount must be greater than 0")]
fn test_delegate_fails_if_zero_amount() {
    // Set up context with correct owner, 10 NEAR balance, and 1 yoctoNEAR deposit
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault with owner.near
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to delegate zero NEAR — should panic
    vault.delegate(
        "validator.poolv1.near".parse().unwrap(),
        NearToken::from_yoctonear(0),
    );
}

#[test]
#[should_panic(expected = "Requested amount")]
fn test_delegate_fails_if_insufficient_balance() {
    // Simulate the vault having exactly 1 NEAR total balance
    // STORAGE_BUFFER (0.01 NEAR) will be subtracted internally
    // So only 0.99 NEAR is available for delegation

    // Attach 1 yoctoNEAR to pass the assert_one_yocto check
    let context = get_context(
        owner(),
        NearToken::from_near(1),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to delegate 1 NEAR — this should panic
    // because get_available_balance will only allow 0.99 NEAR
    vault.delegate(
        "validator.poolv1.near".parse().unwrap(),
        NearToken::from_near(1),
    );
}
