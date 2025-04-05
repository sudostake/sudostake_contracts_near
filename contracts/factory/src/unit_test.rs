#[cfg(test)]
mod tests {
    use crate::contract::FactoryContract;
    use near_sdk::{
        collections::UnorderedMap, test_utils::VMContextBuilder, testing_env, AccountId, NearToken,
    };
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

    #[test]
    fn test_set_vault_creation_fee_by_owner() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let initial_fee = NearToken::from_near(1);
        let new_fee = NearToken::from_near(2);

        let context = VMContextBuilder::new()
            .predecessor_account_id(owner.clone())
            .build();
        testing_env!(context);

        let mut contract = FactoryContract::new(owner.clone(), initial_fee);
        assert_eq!(contract.vault_minting_fee, initial_fee);

        contract.set_vault_creation_fee(new_fee.clone());
        assert_eq!(contract.vault_minting_fee, new_fee);
    }

    #[test]
    #[should_panic(expected = "Only the factory owner can update the vault creation fee")]
    fn test_set_vault_creation_fee_by_non_owner_should_fail() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let attacker: AccountId = "not-owner.near".parse().unwrap();
        let initial_fee = NearToken::from_near(1);
        let new_fee = NearToken::from_near(2);

        let context = VMContextBuilder::new()
            .predecessor_account_id(owner.clone())
            .build();
        testing_env!(context);

        let mut contract = FactoryContract::new(owner.clone(), initial_fee);

        let context = VMContextBuilder::new()
            .predecessor_account_id(attacker.clone())
            .build();
        testing_env!(context);

        contract.set_vault_creation_fee(new_fee);
    }

    #[test]
    fn test_mint_vault_success() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let user: AccountId = "user.near".parse().unwrap();
        let minting_fee = NearToken::from_near(2);
        let attached_deposit = minting_fee.clone();

        // Step 1: Deploy factory with owner context
        let mut owner_context = VMContextBuilder::new();
        owner_context.predecessor_account_id(owner.clone());
        testing_env!(owner_context.build());

        let mut contract = FactoryContract::new(owner.clone(), minting_fee);

        // Step 2: Upload vault code (must be > 1024 bytes)
        let dummy_wasm = vec![1; 2048];
        contract.set_vault_code(dummy_wasm);

        // Step 3: Mint vault with user context
        let mut user_context = VMContextBuilder::new();
        user_context
            .predecessor_account_id(user.clone())
            .attached_deposit(attached_deposit);
        testing_env!(user_context.build());

        contract.mint_vault();

        // Assert vault counter incremented
        assert_eq!(contract.vault_counter, 1);
    }

    #[test]
    #[should_panic(expected = "Vault creation fee has not been set")]
    fn test_mint_vault_fails_if_fee_not_set() {
        // Define test accounts
        let owner: AccountId = "owner.near".parse().unwrap();
        let user: AccountId = "user.near".parse().unwrap();

        // Step 1: Deploy factory with zero minting fee
        let mut owner_context = VMContextBuilder::new();
        owner_context.predecessor_account_id(owner.clone());
        testing_env!(owner_context.build());

        let zero_fee = NearToken::from_yoctonear(0);
        let mut contract = FactoryContract::new(owner.clone(), zero_fee);

        // Step 2: Upload vault code (must be > 1024 bytes)
        let dummy_wasm = vec![1; 2048];
        contract.set_vault_code(dummy_wasm);

        // Step 3: Try to mint vault with user context
        // The attached deposit doesn't matter because fee is zero
        let mut user_context = VMContextBuilder::new();
        user_context
            .predecessor_account_id(user.clone())
            .attached_deposit(NearToken::from_near(1));
        testing_env!(user_context.build());

        // Attempt to mint vault — should panic due to unset fee
        contract.mint_vault();
    }

    #[test]
    #[should_panic(expected = "Must attach exactly the vault minting fee")]
    fn test_mint_vault_fails_if_invalid_fee() {
        // Define test accounts
        let owner: AccountId = "owner.near".parse().unwrap();
        let user: AccountId = "user.near".parse().unwrap();

        // Step 1: Deploy factory with expected minting fee
        let mut owner_context = VMContextBuilder::new();
        owner_context.predecessor_account_id(owner.clone());
        testing_env!(owner_context.build());

        let minting_fee = NearToken::from_near(2);
        let mut contract = FactoryContract::new(owner.clone(), minting_fee.clone());

        // Step 2: Upload valid vault code
        let dummy_wasm = vec![1; 2048];
        contract.set_vault_code(dummy_wasm);

        // Step 3: Attempt to mint vault with user attaching too little deposit
        // Here we intentionally attach 1 NEAR instead of 2
        let mut user_context = VMContextBuilder::new();
        user_context
            .predecessor_account_id(user.clone())
            .attached_deposit(NearToken::from_near(1));
        testing_env!(user_context.build());

        // Should panic due to mismatched deposit
        contract.mint_vault();
    }

    #[test]
    #[should_panic(expected = "No vault code uploaded")]
    fn test_mint_vault_fails_if_no_vault_code() {
        // Define test accounts
        let owner: AccountId = "owner.near".parse().unwrap();
        let user: AccountId = "user.near".parse().unwrap();

        // Step 1: Deploy factory with valid minting fee
        let mut owner_context = VMContextBuilder::new();
        owner_context.predecessor_account_id(owner.clone());
        testing_env!(owner_context.build());

        let minting_fee = NearToken::from_near(2);
        let mut contract = FactoryContract::new(owner.clone(), minting_fee.clone());

        // Step 2: Do NOT upload vault code

        // Step 3: Attempt to mint vault with correct deposit
        let mut user_context = VMContextBuilder::new();
        user_context
            .predecessor_account_id(user.clone())
            .attached_deposit(minting_fee.clone());
        testing_env!(user_context.build());

        // Should panic due to missing vault code
        contract.mint_vault();
    }

    #[test]
    #[should_panic(expected = "Vault minting fee is too low to cover deployment")]
    fn test_mint_vault_fails_if_fee_too_low_to_cover_deploy_and_init() {
        // Define test accounts
        let owner: AccountId = "owner.near".parse().unwrap();
        let user: AccountId = "user.near".parse().unwrap();

        // Step 1: Deploy factory with intentionally low minting fee
        let mut owner_context = VMContextBuilder::new();
        owner_context.predecessor_account_id(owner.clone());
        testing_env!(owner_context.build());

        let low_fee = NearToken::from_yoctonear(1_000_000_000_000_000_000_000); // 0.001 NEAR
        let mut contract = FactoryContract::new(owner.clone(), low_fee.clone());

        // Step 2: Upload large vault code to simulate high deploy cost
        let large_wasm = vec![1; 200 * 1024];
        contract.set_vault_code(large_wasm);

        // Step 3: Attempt to mint vault with insufficient attached deposit
        let mut user_context = VMContextBuilder::new();
        user_context
            .predecessor_account_id(user.clone())
            .attached_deposit(low_fee.clone());
        testing_env!(user_context.build());

        // Call mint_vault — should panic due to insufficient fee
        contract.mint_vault();
    }

    #[test]
    fn test_withdraw_balance_to_self_does_not_panic() {
        // Define the owner and amount to withdraw
        let owner: AccountId = "owner.near".parse().unwrap();
        let amount = NearToken::from_yoctonear(1_000_000_000_000_000_000_000);

        // Set up VM context: owner is caller, with sufficient balance and minimal storage
        let mut context = VMContextBuilder::new();
        context
            .predecessor_account_id(owner.clone())
            .account_balance(NearToken::from_yoctonear(
                10_000_000_000_000_000_000_000_000u128,
            ))
            .storage_usage(100);
        testing_env!(context.build());

        // Initialize factory contract with the owner
        let mut contract = FactoryContract {
            owner: owner.clone(),
            vault_code_versions: UnorderedMap::new(b"v".to_vec()),
            latest_vault_version: 0,
            latest_vault_hash: vec![],
            vault_counter: 0,
            vault_minting_fee: NearToken::from_yoctonear(0),
        };

        // Perform the withdrawal to self
        let _ = contract.withdraw_balance(amount.clone(), None);
    }

    #[test]
    fn test_withdraw_balance_to_third_party_success() {
        use near_sdk::test_utils::VMContextBuilder;
        use near_sdk::{testing_env, AccountId, NearToken, Promise};

        // Setup test accounts
        let owner: AccountId = "owner.near".parse().unwrap();
        let third_party: AccountId = "alice.near".parse().unwrap();

        // Set up VM context with the owner and some balance
        let mut context = VMContextBuilder::new();
        context
            .predecessor_account_id(owner.clone())
            .account_balance(NearToken::from_yoctonear(
                10_000_000_000_000_000_000_000_000_000,
            ));
        testing_env!(context.build());

        // Initialize contract
        let mut contract = FactoryContract {
            owner: owner.clone(),
            vault_code_versions: UnorderedMap::new(b"v".to_vec()),
            latest_vault_version: 0,
            latest_vault_hash: vec![],
            vault_counter: 0,
            vault_minting_fee: NearToken::from_yoctonear(1_000_000_000_000_000_000_000_000),
        };

        // Attempt withdrawal
        let amount = NearToken::from_near(1);
        let promise = contract.withdraw_balance(amount.clone(), Some(third_party.clone()));

        // Assert that we got a Promise back (we cannot check its internals)
        assert!(
            matches!(promise, Promise { .. }),
            "Expected a Promise to be returned"
        );
    }

    #[test]
    #[should_panic(expected = "Only the factory owner can withdraw balance")]
    fn test_withdraw_balance_fails_if_not_owner() {
        // Define a different account (non-owner) and amount to withdraw
        let owner: AccountId = "owner.near".parse().unwrap();
        let non_owner: AccountId = "bob.near".parse().unwrap();
        let amount = NearToken::from_yoctonear(1_000_000_000_000_000_000_000);

        // Set up VM context: non-owner is the caller
        let mut context = VMContextBuilder::new();
        context
            .predecessor_account_id(non_owner.clone())
            .account_balance(NearToken::from_yoctonear(
                10_000_000_000_000_000_000_000_000,
            ))
            .storage_usage(100);
        testing_env!(context.build());

        // Initialize contract with a valid owner (not the caller)
        let mut contract = FactoryContract {
            owner,
            vault_code_versions: UnorderedMap::new(b"v".to_vec()),
            latest_vault_version: 0,
            latest_vault_hash: vec![],
            vault_counter: 0,
            vault_minting_fee: NearToken::from_yoctonear(0),
        };

        // Attempt to withdraw should panic due to caller not being the owner
        let _ = contract.withdraw_balance(amount.clone(), None);
    }

    #[test]
    #[should_panic(expected = "Requested amount exceeds available withdrawable balance")]
    fn test_withdraw_balance_fails_if_exceeds_available_balance() {
        // Define the owner and excessive withdrawal amount
        let owner: AccountId = "owner.near".parse().unwrap();
        let amount = NearToken::from_yoctonear(9_900_000_000_000_000_000_000_000);

        // Set up context with large storage usage (causing low withdrawable balance)
        let mut context = VMContextBuilder::new();
        context
            .predecessor_account_id(owner.clone())
            .account_balance(NearToken::from_yoctonear(
                10_000_000_000_000_000_000_000_000,
            ))
            // large enough to eat up >0.1 NEAR
            .storage_usage(500_000);
        testing_env!(context.build());

        // Initialize contract with the owner
        let mut contract = FactoryContract {
            owner,
            vault_code_versions: UnorderedMap::new(b"v".to_vec()),
            latest_vault_version: 0,
            latest_vault_hash: vec![],
            vault_counter: 0,
            vault_minting_fee: NearToken::from_yoctonear(0),
        };

        // Attempt to withdraw more than allowed should panic
        let _ = contract.withdraw_balance(amount.clone(), None);
    }

    #[test]
    fn test_transfer_ownership_success() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let new_owner: AccountId = "alice.near".parse().unwrap();

        // Step 1: Set up context as the original owner
        let mut context = VMContextBuilder::new();
        context.predecessor_account_id(owner.clone());
        testing_env!(context.build());

        // Step 2: Deploy the contract with the original owner
        let mut contract = FactoryContract::new(owner.clone(), NearToken::from_near(1));

        // Step 3: Transfer ownership to new account
        contract.transfer_ownership(new_owner.clone());

        // Step 4: Verify that the ownership has changed
        assert_eq!(contract.owner, new_owner, "Ownership should be transferred");
    }

    #[test]
    #[should_panic(expected = "Only the factory owner can transfer ownership")]
    fn test_transfer_ownership_fails_if_not_owner() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let attacker: AccountId = "hacker.near".parse().unwrap();
        let new_owner: AccountId = "alice.near".parse().unwrap();

        // Step 1: Set up context as the owner and deploy the contract
        let mut owner_context = VMContextBuilder::new();
        owner_context.predecessor_account_id(owner.clone());
        testing_env!(owner_context.build());

        let mut contract = FactoryContract::new(owner.clone(), NearToken::from_near(1));

        // Step 2: Switch to attacker context to simulate unauthorized caller
        let mut attacker_context = VMContextBuilder::new();
        attacker_context.predecessor_account_id(attacker.clone());
        testing_env!(attacker_context.build());

        // Step 3: Attempt to transfer ownership — should panic
        contract.transfer_ownership(new_owner);
    }

    #[test]
    #[should_panic(expected = "New owner must be different from the current owner")]
    fn test_transfer_ownership_fails_if_same_as_current() {
        let owner: AccountId = "owner.near".parse().unwrap();

        // Step 1: Set up context as the owner
        let mut context = VMContextBuilder::new();
        context.predecessor_account_id(owner.clone());
        testing_env!(context.build());

        // Step 2: Deploy contract
        let mut contract = FactoryContract::new(owner.clone(), NearToken::from_near(1));

        // Step 3: Attempt to transfer ownership to self — should panic
        contract.transfer_ownership(owner);
    }
}
