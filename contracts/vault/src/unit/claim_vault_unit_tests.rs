#[path = "test_utils.rs"]
mod test_utils;

use crate::contract::Vault;
use near_sdk::{test_utils::get_logs, testing_env, NearToken, PromiseError};
use test_utils::{alice, get_context, owner};

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
}

#[test]
#[should_panic(expected = "Vault takeover failed. You may call retry_refunds later.")]
fn test_on_claim_vault_complete_refund_and_panics() {
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
}
