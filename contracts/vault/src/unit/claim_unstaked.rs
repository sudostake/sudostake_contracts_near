#[path = "test_utils.rs"]
mod test_utils;

use crate::contract::Vault;
use near_sdk::{testing_env, AccountId, NearToken};
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
