#[path = "test_utils.rs"]
mod test_utils;

use crate::contract::Vault;
use near_sdk::{json_types::U128, testing_env};
use near_sdk::{AccountId, NearToken};
use test_utils::{alice, get_context, owner};

#[test]
fn owner_can_withdraw_near_successfully() {
    // Set up context with vault owner and 5 NEAR in balance
    let context = get_context(owner(), NearToken::from_near(5), None);
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to withdraw 1 NEAR
    vault.withdraw_balance(
        None, // native NEAR
        U128::from(NearToken::from_near(1).as_yoctonear()),
        None, // default to owner
    );
}

#[test]
#[should_panic(expected = "Not enough NEAR balance")]
fn withdraw_near_insufficient_balance_should_panic() {
    // Set up context with 1 NEAR in the vault
    let context = get_context(owner(), NearToken::from_near(1), None);
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to withdraw more than available (e.g., 5 NEAR)
    vault.withdraw_balance(
        None,
        U128::from(NearToken::from_near(5).as_yoctonear()),
        None,
    );
}

#[test]
#[should_panic(expected = "Only the vault owner can withdraw")]
fn non_owner_cannot_withdraw_should_panic() {
    // Set up context with `alice` as the caller
    // Vault account has 10 NEAR in balance
    let context = get_context(alice(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault with a different owner (`owner`)
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to withdraw 1 NEAR from the vault
    // Since the caller is not the owner, this should panic
    vault.withdraw_balance(
        None,
        U128::from(NearToken::from_near(1).as_yoctonear()),
        None,
    );
}

#[test]
fn owner_can_withdraw_nep141_with_one_yocto() {
    // Set up context with vault owner and attach 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault with owner
    let mut vault = Vault::new(owner(), 0, 1);

    // Use a fake token address for NEP-141 (we're not testing the external contract)
    let fake_token: AccountId = "usdc.mock.near".parse().unwrap();

    // Attempt to withdraw 100 USDC tokens (or whatever you want)
    vault.withdraw_balance(
        Some(fake_token),
        U128::from(100_000_000),
        None, // recipient defaults to owner
    );
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn nep141_withdraw_without_one_yocto_should_panic() {
    // Set up context with vault owner, but no attached deposit
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(0)),
    );
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Use a dummy token address to simulate a NEP-141 withdrawal
    let token: AccountId = "usdc.mock.near".parse().unwrap();

    // Attempt to withdraw tokens without attaching 1 yoctoNEAR
    // This should trigger assert_one_yocto() and panic
    vault.withdraw_balance(
        Some(token),
        U128::from(100_000_000),
        None, // recipient defaults to owner
    );
}
