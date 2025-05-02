#[path = "test_utils.rs"]
mod test_utils;

use crate::contract::Vault;
use near_sdk::{test_utils::get_logs, testing_env, NearToken};
use test_utils::{alice, get_context, owner};

#[test]
fn test_list_for_takeover_success() {
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    assert!(!vault.is_listed_for_takeover);
    vault.list_for_takeover();
    assert!(vault.is_listed_for_takeover);

    let logs = get_logs();
    assert!(
        logs.iter()
            .any(|log| log.contains("vault_listed_for_takeover")),
        "Expected log not found. Logs: {:?}",
        logs
    );
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_list_for_takeover_requires_1yocto() {
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        None, // no deposit
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    vault.list_for_takeover();
}

#[test]
#[should_panic(expected = "Only the vault owner can list the vault for takeover")]
fn test_list_for_takeover_requires_owner() {
    let context = get_context(
        alice(), // not the owner
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    vault.list_for_takeover();
}

#[test]
#[should_panic(expected = "Vault is already listed for takeover")]
fn test_list_for_takeover_fails_if_already_listed() {
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    vault.list_for_takeover();
    vault.list_for_takeover(); // triggers panic
}
