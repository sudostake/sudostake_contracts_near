#[path = "test_utils.rs"]
mod test_utils;

use crate::{
    contract::Vault,
    types::{RefundEntry, REFUND_EXPIRY_EPOCHS},
};
use near_sdk::{
    json_types::U128, test_utils::get_logs, testing_env, AccountId, NearToken, PromiseError,
};
use test_utils::{alice, bob, get_context, insert_refund_entry, owner};

#[test]
fn test_on_refund_complete_success_does_nothing() {
    let context = get_context(alice(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(alice(), 0, 1);

    vault.on_refund_complete(
        alice(),
        U128(1_000_000),
        "usdc.mock.near".parse().unwrap(),
        Ok(()),
    );

    // No entry should be created
    assert!(
        vault.refund_list.is_empty(),
        "Refund list should remain empty on successful refund"
    );
}

#[test]
fn test_on_refund_complete_failure_adds_refund_entry() {
    let context = get_context(alice(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(alice(), 0, 1);

    let proposer = alice();
    let token: AccountId = "usdc.mock.near".parse().unwrap();
    let amount = U128(2_000_000);

    vault.on_refund_complete(
        proposer.clone(),
        amount,
        token.clone(),
        Err(PromiseError::Failed),
    );

    let entry = vault.refund_list.get(&0).expect("Refund entry not added");

    assert_eq!(entry.proposer, proposer);
    assert_eq!(entry.amount, amount);
    assert_eq!(entry.token, Some(token));
}

#[test]
fn test_retry_refunds_owner_can_retry_all_entries() {
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)), // 1 yoctoNEAR attached
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    // Insert two entries with different proposers
    insert_refund_entry(
        &mut vault,
        0,
        RefundEntry {
            proposer: alice(),
            amount: U128(1_000_000),
            token: Some("usdc.mock.near".parse().unwrap()),
            added_at_epoch: 0,
        },
    );
    insert_refund_entry(
        &mut vault,
        1,
        RefundEntry {
            proposer: bob(),
            amount: U128(2_000_000),
            token: None, // plain NEAR refund
            added_at_epoch: 0,
        },
    );

    vault.retry_refunds();

    // Owner should be allowed to retry both: entries must be removed immediately
    assert!(
        vault.refund_list.is_empty(),
        "All refund entries should be removed after retry_refunds by owner"
    );
}

#[test]
fn test_retry_refunds_skips_expired_entries() {
    let mut context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    context.epoch_height = REFUND_EXPIRY_EPOCHS + 5;
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    insert_refund_entry(
        &mut vault,
        0,
        RefundEntry {
            proposer: alice(),
            amount: U128(1_000_000),
            token: None,
            added_at_epoch: 0,
        },
    );

    vault.retry_refunds();

    assert!(
        vault.refund_list.is_empty(),
        "Expired refund entry should be removed"
    );

    let logs = get_logs();
    let found = logs.iter().any(|log| log.contains("retry_refund_expired"));
    assert!(
        found,
        "Expected 'retry_refund_expired' log not found. Logs: {:?}",
        logs
    );
}

#[test]
fn test_retry_refunds_proposer_can_only_retry_their_own_entries() {
    let context = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)), // 1 yoctoNEAR attached
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    // Entry owned by alice (proposer)
    insert_refund_entry(
        &mut vault,
        0,
        RefundEntry {
            proposer: alice(),
            amount: U128(500_000),
            token: None,
            added_at_epoch: 0,
        },
    );

    // Entry owned by bob (not the caller)
    insert_refund_entry(
        &mut vault,
        1,
        RefundEntry {
            proposer: bob(),
            amount: U128(1_000_000),
            token: Some("usdc.mock.near".parse().unwrap()),
            added_at_epoch: 0,
        },
    );

    vault.retry_refunds();

    // Alice’s entry should be removed
    assert!(
        vault.refund_list.get(&0).is_none(),
        "Alice's refund entry should be removed after retry"
    );

    // Bob's entry should remain untouched
    assert!(
        vault.refund_list.get(&1).is_some(),
        "Owner's refund entry should remain"
    );
}

#[test]
#[should_panic(expected = "No refundable entries found for caller")]
fn test_retry_refunds_panics_if_no_entries_found() {
    let context = get_context(
        bob(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)), // 1 yoctoNEAR
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    // Only insert an entry for Alice
    insert_refund_entry(
        &mut vault,
        0,
        RefundEntry {
            proposer: alice(),
            amount: U128(1_000_000),
            token: Some("usdc.mock.near".parse().unwrap()),
            added_at_epoch: 0,
        },
    );

    // Caller is not Alice or owner → should panic
    vault.retry_refunds();
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_retry_refunds_panics_without_one_yocto() {
    // No attached deposit
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    // Even if entries exist, this should panic due to missing 1 yocto
    insert_refund_entry(
        &mut vault,
        0,
        RefundEntry {
            proposer: alice(),
            amount: U128(1_000_000),
            token: None,
            added_at_epoch: 0,
        },
    );

    vault.retry_refunds();
}

#[test]
fn test_on_retry_refund_complete_success_removes_entry() {
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    let refund_id = 42;
    let entry = RefundEntry {
        proposer: alice(),
        amount: U128(750_000),
        token: Some("usdc.mock.near".parse().unwrap()),
        added_at_epoch: 0,
    };

    // Simulate prior removal during schedule_refund
    // Only call the callback
    vault.on_retry_refund_complete(refund_id, entry.clone(), Ok(()));

    // Should NOT re-add the entry
    assert!(
        vault.refund_list.get(&refund_id).is_none(),
        "Refund entry should be permanently removed on success"
    );

    // Ensure success log is emitted
    let logs = get_logs();
    let found = logs
        .iter()
        .any(|log| log.contains("retry_refund_succeeded"));
    assert!(
        found,
        "Expected 'retry_refund_succeeded' log not found. Logs: {:?}",
        logs
    );
}

#[test]
fn test_on_retry_refund_complete_failure_re_adds_if_not_expired() {
    let mut context = get_context(owner(), NearToken::from_near(10), None);
    context.epoch_height = 3; // Current epoch
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    let refund_id = 7;
    let entry = RefundEntry {
        proposer: alice(),
        amount: U128(1_500_000),
        token: None,       // Native NEAR refund
        added_at_epoch: 1, // Only 2 epochs ago
    };

    // Simulate failed retry (entry removed before, now we're in callback)
    vault.on_retry_refund_complete(refund_id, entry.clone(), Err(PromiseError::Failed));

    // Entry should be re-added under the same ID
    let stored = vault.refund_list.get(&refund_id);
    assert!(
        stored.is_some(),
        "Refund entry should be re-added after failed retry if not expired"
    );

    // Check log
    let logs = get_logs();
    let found = logs.iter().any(|log| log.contains("retry_refund_failed"));
    assert!(
        found,
        "Expected 'retry_refund_failed' log not found. Logs: {:?}",
        logs
    );
}

#[test]
fn test_on_retry_refund_complete_failure_discards_if_expired() {
    let mut context = get_context(owner(), NearToken::from_near(10), None);
    context.epoch_height = 20; // Simulate we're far ahead in time
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    let refund_id = 9;
    let entry = RefundEntry {
        proposer: alice(),
        amount: U128(2_000_000),
        token: Some("usdc.mock.near".parse().unwrap()),
        added_at_epoch: 0, // Very old
    };

    // Entry was already removed before the callback
    vault.on_retry_refund_complete(refund_id, entry.clone(), Err(PromiseError::Failed));

    // Entry should NOT be re-added
    let exists = vault.refund_list.get(&refund_id).is_some();
    assert!(
        !exists,
        "Refund entry should be discarded after failed retry if expired"
    );

    // Check log
    let logs = get_logs();
    let found = logs.iter().any(|log| log.contains("retry_refund_failed"));
    assert!(
        found,
        "Expected 'retry_refund_failed' log not found. Logs: {:?}",
        logs
    );
}

#[test]
fn test_on_retry_refund_complete_success_for_near_removes_entry() {
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    let refund_id = 10;
    let entry = RefundEntry {
        proposer: alice(),
        amount: U128(3_000_000),
        token: None, // NEAR refund
        added_at_epoch: 2,
    };

    // Simulate the entry being removed before the callback
    vault.on_retry_refund_complete(refund_id, entry.clone(), Ok(()));

    // Should not be re-added
    assert!(
        vault.refund_list.get(&refund_id).is_none(),
        "NEAR refund entry should be removed on successful retry"
    );

    // Check log
    let logs = get_logs();
    let found = logs
        .iter()
        .any(|log| log.contains("retry_refund_succeeded"));
    assert!(
        found,
        "Expected 'retry_refund_succeeded' log not found. Logs: {:?}",
        logs
    );
}
