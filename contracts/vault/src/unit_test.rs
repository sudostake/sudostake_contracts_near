#[cfg(test)]
mod tests {
    use crate::contract::Vault;
    use near_sdk::{test_utils::VMContextBuilder, testing_env, AccountId};

    fn alice() -> AccountId {
        "alice.near".parse().unwrap()
    }

    #[test]
    fn test_factory_initialization() {
        // Set up mock context
        let context = VMContextBuilder::new()
            .predecessor_account_id(alice())
            .build();
        testing_env!(context);

        // Initialize the contract
        let vault = Vault::new(alice(), 0, 1);

        // Check basic fields
        assert_eq!(vault.owner, alice());
        assert_eq!(vault.index, 0);
        assert_eq!(vault.version, 1);

        // Check validators are initialized as empty
        assert!(vault.active_validators.is_empty());
        assert!(vault.unbonding_validators.is_empty());
        assert!(vault.unstake_entries.is_empty());
    }
}
