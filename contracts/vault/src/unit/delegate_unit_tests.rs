#[path = "test_utils.rs"]
mod test_utils;
use crate::{
    contract::Vault,
    types::{RefundEntry, MAX_ACTIVE_VALIDATORS},
};
use near_sdk::{json_types::U128, test_utils::get_logs, testing_env, AccountId, NearToken};
use test_utils::{alice, get_context, insert_refund_entry, owner};

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
#[should_panic(expected = "You can only stake with")]
fn test_delegate_fails_if_max_active_validators_reached() {
    // Set up context with the vault owner
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Fill active_validators to the limit
    for i in 0..MAX_ACTIVE_VALIDATORS {
        let validator = format!("v{}.poolv1.near", i).parse().unwrap();
        vault.active_validators.insert(&validator);
    }

    // Attempt to add one more validator beyond the limit
    vault.delegate(
        "overflow.poolv1.near".parse().unwrap(),
        NearToken::from_near(1),
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
#[should_panic(expected = "Cannot delegate while there are pending refund entries")]
fn test_delegate_fails_when_refund_list_not_empty() {
    // Setup: 10 NEAR balance, 1 yoctoNEAR attached
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Insert a dummy refund entry
    insert_refund_entry(
        &mut vault,
        0,
        RefundEntry {
            proposer: "alice.near".parse().unwrap(),
            amount: U128(1_000_000),
            token: Some("usdc.mock.near".parse().unwrap()),
            added_at_epoch: 0,
        },
    );

    // Attempt delegation while refund_list is not empty — should panic
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
#[should_panic(expected = "Vault busy with Delegate")]
fn test_delegate_fails_if_lock_active() {
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

    // First call: acquires lock
    vault.delegate(validator.clone(), NearToken::from_near(1));

    // Second call: while lock is still held, should panic
    let another_validator: AccountId = "another.poolv1.near".parse().unwrap();
    vault.delegate(another_validator, NearToken::from_near(1));
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

#[test]
fn test_delegate_allows_existing_validator_even_if_maxed() {
    // Set up context with the vault owner and attach 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Insert MAX_ACTIVE_VALIDATORS validators
    for i in 0..MAX_ACTIVE_VALIDATORS {
        let validator = format!("v{i}.poolv1.near").parse().unwrap();
        vault.active_validators.insert(&validator);
    }

    // Pick one of the existing validators
    let existing_validator: AccountId = "v0.poolv1.near".parse().unwrap();

    // Attempt to delegate to the existing validator — should succeed
    let result = vault.delegate(existing_validator.clone(), NearToken::from_near(1));

    // Confirm we get a Promise (i.e., no panic)
    assert!(matches!(result, near_sdk::Promise { .. }));
}

#[test]
fn test_on_deposit_and_stake_handles_error_without_panic() {
    // Set up context with owner and 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Call delegate() to activate the lock
    vault.delegate(validator.clone(), NearToken::from_near(1));

    // Confirm the lock is now held
    assert_eq!(
        vault.processing_state,
        crate::types::ProcessingState::Delegate,
        "Expected processing_state to be Delegate"
    );

    // Simulate a failed callback
    vault.on_deposit_and_stake(
        validator.clone(),
        NearToken::from_near(1),
        Err(near_sdk::PromiseError::Failed),
    );

    // Validator should NOT be added
    assert!(
        !vault.active_validators.contains(&validator),
        "Validator should not be added on callback failure"
    );

    // Lock should be released
    assert_eq!(
        vault.processing_state,
        crate::types::ProcessingState::Idle,
        "Lock should be released after failed callback"
    );

    // Check logs for delegate_failed
    let logs = get_logs().join("");
    assert!(
        logs.contains("delegate_failed"),
        "Expected log to contain 'delegate_failed'"
    );
}

#[test]
fn test_on_deposit_and_stake_releases_lock_on_success() {
    // Set up context with owner and 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Call delegate to simulate real usage and activate the lock
    vault.delegate(validator.clone(), NearToken::from_near(1));

    // Confirm lock is active
    assert_eq!(
        vault.processing_state,
        crate::types::ProcessingState::Delegate,
        "Expected processing_state to be Delegate"
    );

    // Simulate successful callback
    vault.on_deposit_and_stake(validator.clone(), NearToken::from_near(1), Ok(()));

    // Validator should be added
    assert!(
        vault.active_validators.contains(&validator),
        "Validator should be added on successful callback"
    );

    // Lock should be released
    assert_eq!(
        vault.processing_state,
        crate::types::ProcessingState::Idle,
        "Processing lock should be cleared after success"
    );

    // Check logs for delegate_completed event
    let logs = get_logs().join("");
    assert!(
        logs.contains("delegate_completed"),
        "Expected log to contain 'delegate_completed'"
    );
}
