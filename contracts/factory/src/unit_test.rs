#[cfg(test)]
mod tests {
    use crate::contract::FactoryContract;
    use near_sdk::{test_utils::VMContextBuilder, testing_env, AccountId, NearToken};
    use sha2::{Digest, Sha256};

    #[test]
    fn test_factory_initialization() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let minting_fee = NearToken::from_yoctonear(1_000_000_000_000_000_000_000_000);

        let context = VMContextBuilder::new()
            .signer_account_id(owner.clone())
            .build();
        testing_env!(context);

        let contract = FactoryContract::new(owner.clone(), minting_fee);

        assert_eq!(contract.owner, owner);
        assert_eq!(contract.latest_vault_version, 0);
        assert_eq!(contract.vault_counter, 0);
        assert_eq!(contract.vault_minting_fee, minting_fee);
    }

    #[test]
    fn test_set_vault_code_by_owner() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let minting_fee = NearToken::from_near(1);

        let context = VMContextBuilder::new()
            .predecessor_account_id(owner.clone())
            .build();
        testing_env!(context);

        let mut contract = FactoryContract::new(owner.clone(), minting_fee);
        let code = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]; // small dummy code
        let padded_code = [code.clone(), vec![0u8; 1024]].concat(); // pad to 1KB+

        let expected_hash = Sha256::digest(&padded_code).to_vec();
        let returned_hash = contract.set_vault_code(padded_code.clone());

        assert_eq!(returned_hash, expected_hash);
        assert_eq!(contract.latest_vault_version, 1);
        assert_eq!(contract.latest_vault_hash, expected_hash);
        assert_eq!(
            contract.vault_code_versions.get(&expected_hash),
            Some(padded_code)
        );
    }

    #[test]
    #[should_panic(expected = "Only the factory owner can set new vault code")]
    fn test_set_vault_code_by_non_owner_should_fail() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let not_owner: AccountId = "intruder.near".parse().unwrap();
        let minting_fee = NearToken::from_near(1);

        let context = VMContextBuilder::new()
            .predecessor_account_id(owner.clone())
            .build();
        testing_env!(context);

        let mut contract = FactoryContract::new(owner.clone(), minting_fee);

        // Switch to unauthorized context
        let context = VMContextBuilder::new()
            .predecessor_account_id(not_owner.clone())
            .build();
        testing_env!(context);

        let dummy_code = [vec![1, 2, 3], vec![0u8; 1024]].concat();
        contract.set_vault_code(dummy_code);
    }

    #[test]
    #[should_panic(expected = "This vault code has already been uploaded")]
    fn test_prevent_duplicate_vault_code_upload() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let minting_fee = NearToken::from_near(1);

        let context = VMContextBuilder::new()
            .predecessor_account_id(owner.clone())
            .build();
        testing_env!(context);

        let mut contract = FactoryContract::new(owner.clone(), minting_fee);

        let code = vec![10, 20, 30];
        let padded_code = [code.clone(), vec![0u8; 1024]].concat(); // Ensure >1KB

        // First upload (should succeed)
        contract.set_vault_code(padded_code.clone());

        // Second upload (should panic)
        contract.set_vault_code(padded_code);
    }

    #[test]
    #[should_panic(expected = "Vault code is too small to be valid")]
    fn test_reject_small_vault_code_upload() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let minting_fee = NearToken::from_near(1);

        let context = VMContextBuilder::new()
            .predecessor_account_id(owner.clone())
            .build();
        testing_env!(context);

        let mut contract = FactoryContract::new(owner.clone(), minting_fee);

        let small_code = vec![1, 2, 3, 4, 5]; // much less than 1KB
        contract.set_vault_code(small_code);
    }
}
