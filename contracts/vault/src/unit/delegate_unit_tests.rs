#[path = "test_utils.rs"]
mod test_utils;
use crate::contract::Vault;
use near_sdk::{testing_env, AccountId, NearToken};
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

#[test]
#[should_panic(expected = "Cannot delegate while liquidation is in progress")]
fn test_delegate_fails_if_liquidation_active() {
    // Setup: 10 NEAR balance, 1 yoctoNEAR attached
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Simulate that liquidation has started
    vault.liquidation = Some(crate::types::Liquidation {
        liquidated: NearToken::from_yoctonear(0),
    });

    // Attempt delegation during liquidation — should panic
    vault.delegate(
        "validator.poolv1.near".parse().unwrap(),
        NearToken::from_near(1),
    );
}

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
fn test_delegate_success_does_not_panic() {
    // Set up context with the correct owner and attach 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault owned by owner.near
    let mut vault = Vault::new(owner(), 0, 1);

    // Call delegate with a valid validator and amount
    // This should NOT panic, and return a Promise
    let result = vault.delegate(
        "validator.poolv1.near".parse().unwrap(),
        NearToken::from_near(1),
    );

    // Assert that a Promise is returned (indirectly confirms no panic)
    assert!(matches!(result, near_sdk::Promise { .. }));
}

#[test]
fn test_on_deposit_and_stake_success_adds_validator() {
    // Set up context with the vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define a dummy validator address
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Simulate a successful deposit_and_stake callback
    vault.on_deposit_and_stake(validator.clone(), NearToken::from_near(1), Ok(()));

    // Assert that the validator was added to the active set
    assert!(
        vault.active_validators.contains(&validator),
        "Validator should be marked as active after successful delegation"
    );
}
