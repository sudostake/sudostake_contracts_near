#[path = "test_utils.rs"]
mod test_utils;

use crate::{
    contract::Vault,
    types::{StorageKey, UnstakeEntry, STORAGE_BUFFER},
};
use near_sdk::{
    collections::Vector, env, json_types::U128, test_utils::get_logs, testing_env, AccountId,
    NearToken,
};
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
    vault.on_deposit_and_stake_returned_for_delegate(
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

#[test]
#[should_panic(expected = "Failed to fetch staked balance from validator")]
fn test_on_checked_staked_balance_panics_on_failure() {
    // Set up context with owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Simulate callback from failed get_account_staked_balance
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // This should panic due to simulated callback failure
    vault.on_account_staked_balance_returned_for_undelegate(
        validator,
        NearToken::from_near(1),
        Err(near_sdk::PromiseError::Failed),
    );
}

#[test]
#[should_panic(expected = "Not enough staked balance to undelegate")]
fn test_on_checked_staked_balance_rejects_if_insufficient() {
    // Set up context with owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Simulate callback from get_account_staked_balance with only 0.5 NEAR staked
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();
    let staked_balance = U128::from(500_000_000_000_000_000_000_000u128); // 0.5 NEAR

    // Request to undelegate 1 NEAR (more than staked) — should panic
    vault.on_account_staked_balance_returned_for_undelegate(
        validator,
        NearToken::from_near(1),
        Ok(staked_balance),
    );
}

#[test]
fn test_on_checked_staked_balance_proceeds_on_success() {
    // Set up test context with vault owner and no attached deposit
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault with the correct owner
    let mut vault = Vault::new(owner(), 0, 1);

    // Define the validator we will simulate staking with
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Simulate a successful callback from get_account_staked_balance with 2 NEAR staked
    let staked_balance = U128::from(NearToken::from_near(2).as_yoctonear());

    // Attempt to undelegate 1 NEAR — this should succeed and return a promise
    let _ = vault.on_account_staked_balance_returned_for_undelegate(
        validator.clone(),
        NearToken::from_near(1),
        Ok(staked_balance),
    );

    // Collect logs emitted during the call
    let logs = get_logs();

    // Verify that the log event 'undelegate_check_passed' was emitted
    let found_log = logs
        .iter()
        .any(|log| log.contains("undelegate_check_passed"));

    // Assert that the event log was found
    assert!(
        found_log,
        "Expected log 'undelegate_check_passed' not found. Logs: {:?}",
        logs
    );
}

#[test]
fn test_on_reconciled_unstake_handles_successful_withdrawal() {
    // Set up test context with vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define the validator account
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Simulate one existing unstake entry of 1 NEAR
    let unstake_amount = NearToken::from_near(1).as_yoctonear();
    let mut queue = Vector::new(StorageKey::UnstakeEntriesPerValidator {
        validator_hash: env::sha256(validator.as_bytes()),
    });
    queue.push(&UnstakeEntry {
        amount: unstake_amount,
        epoch_height: 100,
    });

    // Insert the queue into vault state
    vault.unstake_entries.insert(&validator, &queue);

    // Simulate get_account_unstaked_balance callback returning 0 NEAR remaining
    let remaining_unstaked = U128::from(0);

    // Call the method — should reconcile and proceed to unstake
    let _ = vault.on_account_unstaked_balance_returned_for_undelegate(
        validator.clone(),
        NearToken::from_near(1),
        false,
        Ok(remaining_unstaked),
    );

    // Collect logs emitted during reconciliation
    let logs = get_logs();

    // Verify unstake_initiated log was emitted
    let found_unstake = logs.iter().any(|log| log.contains("unstake_initiated"));
    assert!(found_unstake, "Expected log 'unstake_initiated' not found");
}

#[test]
fn test_on_reconciled_unstake_handles_extra_rewards() {
    // Set up the test context with the vault owner and no attached deposit
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault with the owner account
    let mut vault = Vault::new(owner(), 0, 1);

    // Define the validator to simulate unbonding from
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Create a new unstake entry queue with one 1 NEAR entry
    let mut queue = Vector::new(StorageKey::UnstakeEntriesPerValidator {
        validator_hash: env::sha256(validator.as_bytes()),
    });

    // Push a single unstake entry to the queue
    queue.push(&UnstakeEntry {
        amount: 1_000_000_000_000_000_000_000_000,
        epoch_height: 100,
    });

    // Insert the queue into vault state for the validator
    vault.unstake_entries.insert(&validator, &queue);

    // Simulate a callback where all NEAR was withdrawn, leaving 0 remaining
    let remaining_unstaked = U128::from(0);

    // Call the method — this should trigger reconciliation and continue unstaking
    let _promise = vault.on_account_unstaked_balance_returned_for_undelegate(
        validator.clone(),
        NearToken::from_near(2),
        false,
        Ok(remaining_unstaked),
    );

    // Assert that the validator's unstake entry queue has been cleared
    assert!(
        vault.unstake_entries.get(&validator).is_none(),
        "Expected unstake_entries to be cleared for validator"
    );
}

#[test]
#[should_panic(expected = "Failed to fetch unstaked balance from validator")]
fn test_on_reconciled_unstake_panics_on_failure() {
    // Set up the test context with the vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault with the correct owner
    let mut vault = Vault::new(owner(), 0, 1);

    // Define the validator for this undelegation flow
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Attempt to call the reconciled callback with a simulated failure
    vault.on_account_unstaked_balance_returned_for_undelegate(
        validator,
        NearToken::from_near(1),
        false,
        Err(near_sdk::PromiseError::Failed),
    );
}

#[test]
fn test_on_unstake_complete_adds_unstake_entry() {
    // Set up test context with vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define the validator to simulate unstaking from
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Simulate a successful callback from unstake() by passing Ok(())
    vault.on_unstake_returned_for_undelegate(
        validator.clone(),
        NearToken::from_near(1),
        false,
        Ok(()),
    );

    // Fetch the queue from state after the call
    let queue = vault
        .unstake_entries
        .get(&validator)
        .expect("Validator queue should exist");

    // Assert that one entry exists
    assert_eq!(queue.len(), 1, "Expected one unstake entry in the queue");

    // Fetch the entry and assert it matches expected amount
    let entry = queue.get(0).unwrap();
    assert_eq!(
        entry.amount,
        NearToken::from_near(1).as_yoctonear(),
        "Unstake entry amount is incorrect"
    );

    // Assert that the epoch_height was recorded as the current block epoch
    assert_eq!(
        entry.epoch_height,
        env::epoch_height(),
        "Epoch height recorded is incorrect"
    );
}

#[test]
#[should_panic(expected = "Failed to execute unstake on validator")]
fn test_on_unstake_complete_panics_on_failure() {
    // Set up test context with vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define the validator we are simulating unstaking from
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Simulate a failed callback from the unstake() Promise
    vault.on_unstake_returned_for_undelegate(
        validator,
        NearToken::from_near(1),
        false,
        Err(near_sdk::PromiseError::Failed),
    );
}

#[test]
fn test_on_unstake_complete_removes_validator_when_flag_is_true() {
    // Set up test context with vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define the validator
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Manually add the validator to the active set
    vault.active_validators.insert(&validator);

    // Ensure validator is initially active
    assert!(
        vault.active_validators.contains(&validator),
        "Validator should be initially active"
    );

    // Simulate successful unstake callback with removal flag
    vault.on_unstake_returned_for_undelegate(
        validator.clone(),
        NearToken::from_near(1),
        true,
        Ok(()),
    );

    // Assert that the validator has been removed from active_validators
    assert!(
        !vault.active_validators.contains(&validator),
        "Validator should have been removed from active_validators"
    );
}

#[test]
fn test_on_withdraw_all_returned_triggers_balance_check() {
    // Set up context with the vault owner
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Define the validator account
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Call the internal method and capture the returned Promise
    let _ = vault.on_withdraw_all_returned_for_claim_unstaked(validator.clone());
}

#[test]
fn test_on_account_unstaked_balance_returned_for_claim_unstaked_success() {
    // Set up the execution context with the vault owner and 10 NEAR balance
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Define the validator account we'll use in the test
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Initialize a new Vault contract instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Create an unstake entry for 1 NEAR
    let mut queue = Vector::new(StorageKey::UnstakeEntriesPerValidator {
        validator_hash: env::sha256(validator.as_bytes()),
    });

    // Push a single entry with 1 NEAR and dummy epoch height
    queue.push(&UnstakeEntry {
        amount: NearToken::from_near(1).as_yoctonear(),
        epoch_height: 100,
    });

    // Store the queue in the vault under the validator
    vault.unstake_entries.insert(&validator, &queue);

    // Simulate that the validator now reports 0 NEAR remaining (meaning 1 NEAR was withdrawn)
    let remaining_unstaked = U128::from(0);

    // Call the method to simulate the callback resolution
    vault.on_account_unstaked_balance_returned_for_claim_unstaked(
        validator.clone(),
        Ok(remaining_unstaked),
    );

    // Collect emitted logs
    let logs = get_logs();

    // Check that the claim_unstaked_completed log was emitted
    let found_claimed = logs
        .iter()
        .any(|log| log.contains("claim_unstaked_completed"));

    // Assert that the claim completion log was found
    assert!(
        found_claimed,
        "Expected 'claim_unstaked_completed' log not found"
    );

    // Confirm that the validator's unstake entry queue has been fully cleared
    assert!(
        vault.unstake_entries.get(&validator).is_none(),
        "Expected unstake_entries to be cleared for validator"
    );
}

#[test]
fn test_on_account_unstaked_balance_returned_for_claim_unstaked_partial() {
    // Set up the vault environment with the owner and 10 NEAR balance
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Define the validator for the test
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Initialize the vault contract
    let mut vault = Vault::new(owner(), 0, 1);

    // Create a queue with two unstake entries:
    // - Entry A: 0.4 NEAR
    // - Entry B: 0.6 NEAR
    let mut queue = Vector::new(StorageKey::UnstakeEntriesPerValidator {
        validator_hash: env::sha256(validator.as_bytes()),
    });

    let entry_a = UnstakeEntry {
        amount: 400_000_000_000_000_000_000_000, // 0.4 NEAR
        epoch_height: 100,
    };

    let entry_b = UnstakeEntry {
        amount: 600_000_000_000_000_000_000_000, // 0.6 NEAR
        epoch_height: 101,
    };

    queue.push(&entry_a);
    queue.push(&entry_b);

    // Insert the queue into the vault for the validator
    vault.unstake_entries.insert(&validator, &queue);

    // Simulate a withdrawal of only 0.4 NEAR (matching entry_a)
    // Only entry_b should remain
    let remaining_unstaked = U128::from(entry_b.amount);

    // Trigger the reconciliation callback with the partial withdrawal
    vault.on_account_unstaked_balance_returned_for_claim_unstaked(
        validator.clone(),
        Ok(remaining_unstaked),
    );

    // Get the updated unstake queue from contract state
    let updated_queue = vault
        .unstake_entries
        .get(&validator)
        .expect("Queue should still exist after partial reconciliation");

    // Ensure only one entry remains in the queue
    assert_eq!(
        updated_queue.len(),
        1,
        "Expected one entry to remain in the unstake queue"
    );

    // Fetch the remaining entry
    let remaining_entry = updated_queue.get(0).unwrap();

    // Ensure it is the correct (second) entry
    assert_eq!(
        remaining_entry.amount, entry_b.amount,
        "Remaining entry should match entry_b"
    );
    assert_eq!(
        remaining_entry.epoch_height, entry_b.epoch_height,
        "Epoch height of remaining entry should match entry_b"
    );
}

#[test]
#[should_panic(expected = "Failed to fetch unstaked balance from validator")]
fn test_on_account_unstaked_balance_returned_for_claim_unstaked_failure() {
    // Set up context with vault owner and balance
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Define the validator for this test
    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Simulate callback failure from get_account_unstaked_balance
    vault.on_account_unstaked_balance_returned_for_claim_unstaked(
        validator,
        Err(near_sdk::PromiseError::Failed),
    );
}
