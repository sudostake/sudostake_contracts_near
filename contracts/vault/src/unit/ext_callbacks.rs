#[path = "test_utils.rs"]
mod test_utils;

use crate::{contract::Vault, types::STORAGE_BUFFER};
use near_sdk::{testing_env, NearToken};
use test_utils::{get_context, owner};

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
