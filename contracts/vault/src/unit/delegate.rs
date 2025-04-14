#[path = "test_utils.rs"]
mod test_utils;
use crate::{
    contract::Vault,
    types::{StorageKey, UnstakeEntry},
};
use near_sdk::{collections::Vector, env, test_utils::get_logs, testing_env, AccountId, NearToken};
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
fn test_delegate_direct_executes_if_no_unstake_entries() {
    // Context: vault has 2 NEAR and attached 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(2),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault owned by owner.near
    let mut vault = Vault::new(owner(), 0, 1);

    // Ensure the validator has no unstake entries
    assert!(vault
        .unstake_entries
        .get(&"validator.poolv1.near".parse().unwrap())
        .is_none());

    // Attempt to delegate 1 NEAR
    let _promise = vault.delegate(
        "validator.poolv1.near".parse().unwrap(),
        NearToken::from_near(1),
    );

    // Check validator is now tracked
    vault.on_deposit_and_stake_returned_for_delegate(
        "validator.poolv1.near".parse().unwrap(),
        NearToken::from_near(1),
        Ok(()),
    );
    assert!(vault
        .active_validators
        .contains(&"validator.poolv1.near".parse().unwrap()));

    // Verify that the delegate_direct event was logged
    let logs = get_logs();
    let found_log = logs.iter().any(|log| log.contains("delegate_direct"));
    assert!(found_log, "Expected 'delegate_direct' log not found");
}

#[test]
fn test_delegate_goes_through_withdraw_if_unstake_entries_exist() {
    // Setup test environment with:
    // - Contract account balance: 2 NEAR
    // - Attached deposit: 1 yoctoNEAR (required by assert_one_yocto)
    // - Caller: owner.near
    let context = get_context(
        owner(),
        NearToken::from_near(2),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // The validator we will delegate to
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Initialize a new vault owned by `owner.near`
    let mut vault = Vault::new(owner(), 0, 1);

    // Manually add a dummy unstake entry for the validator
    // This simulates the presence of unclaimed unbonded tokens
    let mut queue = Vector::new(StorageKey::UnstakeEntriesPerValidator {
        validator_hash: env::sha256(validator.as_bytes()),
    });
    queue.push(&UnstakeEntry {
        amount: NearToken::from_near(1).as_yoctonear(),
        epoch_height: 100,
    });
    vault.unstake_entries.insert(&validator, &queue);

    // Call delegate with 1 NEAR
    // Because unstake_entries exist, the vault should go through:
    //   withdraw_all → reconcile → deposit_and_stake
    // NOT the fast path (delegate_direct)
    let _ = vault.delegate(validator.clone(), NearToken::from_near(1));

    // Inspect emitted logs
    // Should contain "delegate_started" but not "delegate_direct"
    let logs = get_logs();
    let found_delegate_direct = logs.iter().any(|log| log.contains("delegate_direct"));
    let found_delegate_started = logs.iter().any(|log| log.contains("delegate_started"));

    assert!(
        !found_delegate_direct,
        "Should not log 'delegate_direct' when unstake entries exist"
    );

    assert!(
        found_delegate_started,
        "Expected 'delegate_started' log not found"
    );
}
