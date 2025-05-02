#[path = "test_utils.rs"]
mod test_utils;

use crate::contract::Vault;
use near_sdk::{test_utils::get_logs, testing_env, NearToken};
use test_utils::{alice, get_context, owner};

#[test]
fn test_cancel_takeover_success() {
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    vault.is_listed_for_takeover = true;

    vault.cancel_takeover();

    assert!(
        !vault.is_listed_for_takeover,
        "Vault should no longer be listed"
    );
    let logs = get_logs();
    let found = logs
        .iter()
        .any(|log| log.contains("vault_takeover_cancelled"));
    assert!(
        found,
        "Expected 'vault_takeover_cancelled' log not found. Logs: {:?}",
        logs
    );
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_cancel_takeover_requires_1yocto() {
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        None, // no yoctoNEAR
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    vault.is_listed_for_takeover = true;

    vault.cancel_takeover(); // should panic
}

#[test]
#[should_panic(expected = "Only the vault owner can cancel takeover")]
fn test_cancel_takeover_rejects_non_owner() {
    let context = get_context(
        alice(), // not the owner
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    vault.is_listed_for_takeover = true;

    vault.cancel_takeover(); // should panic
}

#[test]
#[should_panic(expected = "Vault is not listed for takeover")]
fn test_cancel_takeover_fails_if_not_listed() {
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    vault.cancel_takeover(); // should panic
}
