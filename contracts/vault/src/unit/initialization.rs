#[path = "test_utils.rs"]
mod test_utils;
use near_sdk::{testing_env, NearToken};
use test_utils::{alice, get_context};

use crate::Vault;

#[test]
fn test_vault_initialization() {
    // Set up mock context
    let context = get_context(alice(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize the contract
    let vault = Vault::new(alice(), 0, 1);

    // Check basic fields
    assert_eq!(vault.owner, alice());
    assert_eq!(vault.index, 0);
    assert_eq!(vault.version, 1);

    // Check validators are initialized as empty
    assert!(vault.active_validators.is_empty());
    assert!(vault.unstake_entries.is_empty());
}
