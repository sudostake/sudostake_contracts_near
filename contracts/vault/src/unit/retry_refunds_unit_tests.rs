#[path = "test_utils.rs"]
mod test_utils;

use crate::{contract::Vault, types::RefundEntry};
use near_sdk::{
    json_types::U128, test_utils::get_logs, testing_env, AccountId, NearToken, PromiseError,
};
use test_utils::{alice, get_context, insert_refund_entry, owner};

#[test]
fn test_on_refund_complete_does_nothing_on_success() {
    let context = get_context(alice(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(alice(), 0, 1);

    vault.on_refund_complete(
        alice(),
        U128(1_000_000),
        "usdc.mock.near".parse().unwrap(),
        Ok(()), // Simulate successful promise
    );

    assert_eq!(
        vault.refund_list.len(),
        0,
        "Expected refund_list to remain empty on successful refund"
    );
}

#[test]
fn test_on_refund_complete_adds_entry_on_failure() {
    let context = get_context(alice(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(alice(), 0, 1);

    let proposer = alice();
    let token: AccountId = "usdc.mock.near".parse().unwrap();
    let amount = U128(1_000_000);

    // Simulate a failed promise
    vault.on_refund_complete(
        proposer.clone(),
        amount,
        token.clone(),
        Err(PromiseError::Failed),
    );

    // Expect one refund entry
    assert_eq!(vault.refund_list.len(), 1, "Expected one refund entry");

    let stored = vault.refund_list.get(&0).expect("Refund entry missing");

    assert_eq!(stored.proposer, proposer, "Proposer mismatch");
    assert_eq!(stored.amount.0, amount.0, "Amount mismatch");
    assert_eq!(
        stored.token.as_ref(),
        Some(&token),
        "Token address mismatch"
    );

    // Verify log contains refund_failed
    let logs = get_logs();
    let matched = logs.iter().any(|log| log.contains("refund_failed"));
    assert!(matched, "Expected refund_failed log not found: {:?}", logs);
}

#[test]
fn test_retry_refunds_by_owner_filters_entries_and_schedules() {
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    // Insert entries: one for owner, one for alice
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
            proposer: owner(),
            amount: U128(2_000_000),
            token: Some("usdc.mock.near".parse().unwrap()),
            added_at_epoch: 0,
        },
    );

    // Owner should be able to retry both
    vault.retry_refunds();

    // We don't assert state changes here because actual refunds are async.
    // But we can assert that no panic occurred and refund_list length is unchanged
    assert_eq!(
        vault.refund_list.len(),
        2,
        "Entries should not be removed during retry; only in callback"
    );
}

#[test]
fn test_retry_refunds_by_proposer_filters_entries_and_schedules() {
    let context = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    // Entry owned by alice (proposer)
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

    // Entry not owned by alice
    insert_refund_entry(
        &mut vault,
        1,
        RefundEntry {
            proposer: owner(),
            amount: U128(2_000_000),
            token: Some("usdc.mock.near".parse().unwrap()),
            added_at_epoch: 0,
        },
    );

    // alice should only be able to retry her own entry
    vault.retry_refunds();

    // Confirm that refund_list still contains both entries (retry doesn't remove)
    assert_eq!(
        vault.refund_list.len(),
        2,
        "Entries should remain after retry attempt"
    );
}

#[test]
#[should_panic(expected = "No refundable entries found for caller")]
fn test_retry_refunds_panics_when_no_entries_found() {
    let context = get_context(
        "random.near".parse().unwrap(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    // Entry only for alice (not for the caller)
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

    vault.retry_refunds(); // should panic
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_retry_refunds_requires_one_yocto() {
    // No attached deposit
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    // Should panic due to missing 1 yocto
    vault.retry_refunds();
}

#[test]
fn test_on_retry_refund_complete_succeeds_and_removes_entry() {
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    // Insert a pending refund entry
    insert_refund_entry(
        &mut vault,
        42,
        RefundEntry {
            proposer: alice(),
            amount: U128(1_000_000),
            token: Some("usdc.mock.near".parse().unwrap()),
            added_at_epoch: 0,
        },
    );

    // Simulate successful retry
    vault.on_retry_refund_complete(42, Ok(()));

    // Entry should be removed
    assert!(
        vault.refund_list.get(&42).is_none(),
        "Expected refund entry to be removed on success"
    );

    // Log should contain `retry_refund_succeeded`
    let logs = get_logs();
    let found = logs
        .iter()
        .any(|log| log.contains("retry_refund_succeeded"));
    assert!(
        found,
        "Expected retry_refund_succeeded log not found. Logs: {:?}",
        logs
    );
}

#[test]
fn test_on_retry_refund_failed() {
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    insert_refund_entry(
        &mut vault,
        99,
        RefundEntry {
            proposer: alice(),
            amount: U128(1_000_000),
            token: Some("usdc.mock.near".parse().unwrap()),
            added_at_epoch: 0,
        },
    );

    vault.on_retry_refund_complete(99, Err(PromiseError::Failed));

    let logs = get_logs();
    let found = logs.iter().any(|log| log.contains("retry_refund_failed"));
    assert!(
        found,
        "Expected 'retry_refund_failed' event not found. Logs: {:?}",
        logs
    );
}

#[test]
fn test_remove_refund_entry_if_expired_removes_entry() {
    let mut context = get_context(owner(), NearToken::from_near(10), None);
    context.epoch_height = 15;
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    let refund_nonce = 5;
    insert_refund_entry(
        &mut vault,
        refund_nonce,
        RefundEntry {
            proposer: alice(),
            amount: U128(123),
            token: None,
            added_at_epoch: 10, // 5 epochs ago
        },
    );

    vault.on_retry_refund_complete(refund_nonce, Err(PromiseError::Failed));

    assert!(
        vault.refund_list.get(&refund_nonce).is_none(),
        "Expected expired refund to be removed"
    );

    let logs = get_logs();
    let found = logs.iter().any(|log| log.contains("refund_removed"));
    assert!(
        found,
        "Expected 'refund_removed' log not found. Logs: {:?}",
        logs
    );
}

#[test]
fn test_on_retry_refund_complete_keeps_non_expired_entry() {
    // Simulate we're at epoch 12 (entry added at 10 → not yet expired)
    let mut context = get_context(owner(), NearToken::from_near(10), None);
    context.epoch_height = 12;
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    let refund_nonce = 5;
    insert_refund_entry(
        &mut vault,
        refund_nonce,
        RefundEntry {
            proposer: alice(),
            amount: U128(2_000),
            token: None,
            added_at_epoch: 10, // only 2 epochs old
        },
    );

    vault.on_retry_refund_complete(refund_nonce, Err(PromiseError::Failed));

    assert!(
        vault.refund_list.get(&refund_nonce).is_some(),
        "Expected refund entry to remain (not expired)"
    );

    let logs = get_logs();
    let found = logs.iter().any(|log| log.contains("retry_refund_failed"));
    assert!(
        found,
        "Expected 'retry_refund_failed' log not found. Logs: {:?}",
        logs
    );
}

#[test]
fn test_on_retry_refund_complete_noop_if_entry_missing() {
    let mut context = get_context(owner(), NearToken::from_near(10), None);
    context.epoch_height = 20;
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    // Do not insert any refund entry
    vault.on_retry_refund_complete(999, Err(PromiseError::Failed));

    // refund_list should remain empty
    assert_eq!(
        vault.refund_list.len(),
        0,
        "Expected refund_list to remain empty"
    );

    let logs = get_logs();
    let found = logs.iter().any(|log| log.contains("retry_refund_failed"));
    assert!(
        found,
        "Expected 'retry_refund_failed' log not found. Logs: {:?}",
        logs
    );
}
