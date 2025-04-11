#[path = "test_utils.rs"]
mod test_utils;
use crate::{StorageKey, UnstakeEntry, Vault};
use near_sdk::{collections::Vector, env, testing_env, AccountId, NearToken};
use test_utils::{get_context, owner};

#[test]
fn test_reconcile_unstake_entries_clears_fully_withdrawn_queue() {
    // Set up test context with owner and enough balance
    let context = get_context(owner(), NearToken::from_near(2), None);
    testing_env!(context);

    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add two unstake entries totaling 1 NEAR
    let mut queue = Vector::new(StorageKey::UnstakeEntriesPerValidator {
        validator_hash: env::sha256(validator.as_bytes()),
    });
    queue.push(&UnstakeEntry {
        amount: 400_000_000_000_000_000_000_000,
        epoch_height: 100,
    });
    queue.push(&UnstakeEntry {
        amount: 600_000_000_000_000_000_000_000,
        epoch_height: 101,
    });
    vault.unstake_entries.insert(&validator, &queue);

    // Reconcile with full withdrawal of 1 NEAR
    vault.reconcile_unstake_entries(&validator, NearToken::from_near(1).as_yoctonear());

    // After reconciliation, both entries should be removed
    assert!(
        vault.unstake_entries.get(&validator).is_none(),
        "Unstake entry map should not contain validator"
    );
}

#[test]
fn test_reconcile_unstake_entries_partial_removal() {
    // Set up test context with owner and enough balance
    let context = get_context(owner(), NearToken::from_near(2), None);
    testing_env!(context);

    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Initialize vault and add 2 unstake entries
    let mut vault = Vault::new(owner(), 0, 1);

    // Add two unstake entries totaling 1 NEAR
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
    vault.unstake_entries.insert(&validator, &queue);

    // Simulate withdrawing only 0.4 NEAR
    vault.reconcile_unstake_entries(&validator, entry_a.amount);

    // Ensure:
    // - entry_a is removed
    // - entry_b is still present
    // - validator still tracked in unstaked_entries
    let new_queue = vault
        .unstake_entries
        .get(&validator)
        .expect("Queue should still exist");
    let remaining_entries: Vec<_> = new_queue.iter().collect();
    assert_eq!(remaining_entries.len(), 1, "Only one entry should remain");
    assert_eq!(
        remaining_entries[0].amount, entry_b.amount,
        "Remaining entry should match entry_b"
    );
}

#[test]
fn test_reconcile_unstake_entries_handles_extra_rewards() {
    // Setup context with 2 NEAR balance
    let context = get_context(owner(), NearToken::from_near(2), None);
    testing_env!(context);

    let validator: AccountId = "validator.poolv1.near".parse().unwrap();

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add one unstake entry for 1 NEAR
    let mut queue = Vector::new(StorageKey::UnstakeEntriesPerValidator {
        validator_hash: env::sha256(validator.as_bytes()),
    });
    let entry = UnstakeEntry {
        amount: NearToken::from_near(1).as_yoctonear(),
        epoch_height: 100,
    };
    queue.push(&entry);
    vault.unstake_entries.insert(&validator, &queue);

    // Simulate total withdrawn = 1.5 NEAR (rewards included)
    vault.reconcile_unstake_entries(&validator, 1_500_000_000_000_000_000_000_000);

    // Validate that the unstake entry was removed
    assert!(
        vault.unstake_entries.get(&validator).is_none(),
        "Unstake entry should be cleared"
    );
}
