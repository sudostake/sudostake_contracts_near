#[path = "test_utils.rs"]
mod test_utils;

use crate::{
    contract::Vault,
    types::{ProcessingState, RefundEntry},
};
use near_sdk::{test_utils::get_logs, testing_env, NearToken, PromiseError};
use test_utils::{alice, carol, get_context, owner};

#[test]
fn test_claim_vault_starts_promise_transfer() {
    let mut vault = Vault::new(owner(), 0, 1);
    vault.is_listed_for_takeover = true;

    let required_deposit = NearToken::from_yoctonear(vault.get_storage_cost());

    let context = get_context(
        alice(), // not the owner
        NearToken::from_near(10),
        Some(required_deposit),
    );
    testing_env!(context);

    // Should not panic
    let _ = vault.claim_vault();

    assert_eq!(
        vault.processing_state,
        ProcessingState::ClaimVault,
        "Vault should remain locked until callback resolves"
    );
}

#[test]
#[should_panic(expected = "Vault is not listed for takeover")]
fn test_claim_vault_rejects_if_not_listed() {
    let mut vault = Vault::new(owner(), 0, 1);
    let required_deposit = NearToken::from_yoctonear(vault.get_storage_cost());

    let context = get_context(
        alice(), // not the owner
        NearToken::from_near(10),
        Some(required_deposit),
    );
    testing_env!(context);

    // Should panic
    let _ = vault.claim_vault();
}

#[test]
#[should_panic(expected = "Vault busy with ClaimVault")]
fn test_claim_vault_rejects_if_pending_claim_exists() {
    let mut vault = Vault::new(owner(), 0, 1);
    vault.is_listed_for_takeover = true;
    vault.processing_state = ProcessingState::ClaimVault;

    let required_deposit = NearToken::from_yoctonear(vault.get_storage_cost());

    let context = get_context(alice(), NearToken::from_near(10), Some(required_deposit));
    testing_env!(context);

    let _ = vault.claim_vault();
}

#[test]
#[should_panic(expected = "Current vault owner cannot claim their own vault")]
fn test_claim_vault_rejects_self_claim() {
    let mut vault = Vault::new(owner(), 0, 1);
    vault.is_listed_for_takeover = true;

    let required_deposit = NearToken::from_yoctonear(vault.get_storage_cost());

    let context = get_context(
        owner(), // owner of vault
        NearToken::from_near(10),
        Some(required_deposit),
    );
    testing_env!(context);

    // Should panic
    let _ = vault.claim_vault();
}

#[test]
#[should_panic(expected = "Must attach exactly")]
fn test_claim_vault_rejects_wrong_deposit() {
    let context = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_near(1)),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    vault.is_listed_for_takeover = true;
    vault.claim_vault();
}

#[test]
fn test_on_claim_vault_complete_succeeds() {
    let old_owner = owner();
    let new_owner = alice();
    let amount: u128 = 10_000_000_000_000_000_000_000; // 10 NEAR in yocto

    let context = get_context(old_owner.clone(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(old_owner.clone(), 0, 1);
    vault.is_listed_for_takeover = true;

    vault.on_claim_vault_complete(
        old_owner.clone(),
        new_owner.clone(),
        amount,
        Ok(()), // Simulate successful transfer
    );

    assert_eq!(vault.owner, new_owner, "Ownership was not transferred");
    assert!(
        !vault.is_listed_for_takeover,
        "Vault should not be listed anymore"
    );

    let logs = get_logs();
    let found = logs.iter().any(|log| log.contains("vault_claimed"));
    assert!(
        found,
        "Expected 'vault_claimed' event not found. Logs: {:?}",
        logs
    );

    assert_eq!(
        vault.processing_state,
        ProcessingState::Idle,
        "Processing lock should be released after success"
    );
}

#[test]
fn test_on_claim_vault_failed() {
    let old_owner = owner();
    let new_owner = alice();
    let amount: u128 = 10_000_000_000_000_000_000_000;

    let context = get_context(old_owner.clone(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(old_owner.clone(), 0, 1);
    vault.is_listed_for_takeover = true;

    vault.on_claim_vault_complete(
        old_owner.clone(),
        new_owner.clone(),
        amount,
        Err(PromiseError::Failed), // Simulate failed transfer
    );

    // Inspect logs
    let logs = get_logs();
    let found = logs.iter().any(|log| log.contains("claim_vault_failed"));
    assert!(
        found,
        "Expected 'claim_vault_failed' event not found. Logs: {:?}",
        logs
    );

    // Refund should be stored
    let refunds: Vec<(u64, RefundEntry)> = vault.refund_list.iter().collect();
    assert_eq!(refunds.len(), 1, "Expected one refund entry");

    let (_, refund) = &refunds[0];
    assert_eq!(refund.token, None, "Refund should be in native NEAR");
    assert_eq!(
        &refund.proposer, &new_owner,
        "Refund should go to the new_owner"
    );
    assert_eq!(
        refund.amount.0, amount,
        "Refund amount should match the takeover price"
    );

    assert_eq!(
        vault.processing_state,
        ProcessingState::Idle,
        "Processing lock should be released after failure"
    );
}

#[test]
fn test_on_claim_vault_stale_relists_and_refunds() {
    let old_owner = owner();
    let new_owner = alice();
    let replacement_owner = carol();
    let amount: u128 = 5_000_000_000_000_000_000_000;

    let context = get_context(old_owner.clone(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(old_owner.clone(), 0, 1);
    vault.is_listed_for_takeover = true;
    vault.processing_state = ProcessingState::ClaimVault;

    // Simulate an external ownership change while the transfer was in flight
    vault.owner = replacement_owner.clone();

    vault.on_claim_vault_complete(old_owner.clone(), new_owner.clone(), amount, Ok(()));

    assert_eq!(
        vault.owner, replacement_owner,
        "Ownership should remain with the latest on-chain owner"
    );
    assert!(
        vault.is_listed_for_takeover,
        "Vault should be relisted after stale claim"
    );

    let refunds: Vec<(u64, RefundEntry)> = vault.refund_list.iter().collect();
    assert_eq!(
        refunds.len(),
        1,
        "Refund entry should be recorded for claimant"
    );
    let (_, refund) = &refunds[0];
    assert_eq!(refund.proposer, new_owner, "Refund must target claimant");
    assert_eq!(
        refund.amount.0, amount,
        "Refund amount should match takeover price"
    );

    assert_eq!(
        vault.processing_state,
        ProcessingState::Idle,
        "Processing lock should be released after stale resolution"
    );
}
