#[cfg(test)]
mod tests {
    use crate::contract::{StorageKey, UnstakeEntry, Vault, STORAGE_BUFFER};
    use near_sdk::{
        env,
        test_utils::{get_logs, VMContextBuilder},
        testing_env, AccountId, NearToken,
    };

    use near_sdk::collections::Vector;

    fn alice() -> AccountId {
        "alice.near".parse().unwrap()
    }

    fn owner() -> AccountId {
        "owner.near".parse().unwrap()
    }

    fn get_context(
        predecessor: AccountId,
        account_balance: NearToken,
        attached_deposit: Option<NearToken>,
    ) -> near_sdk::VMContext {
        // Step 1: Create a mutable context builder
        let mut builder = VMContextBuilder::new();

        // Step 2: Set the signer and account balance directly on the mutable builder
        builder.predecessor_account_id(predecessor);
        builder.account_balance(account_balance);

        // Step 3: Set attached deposit if provided
        if let Some(deposit) = attached_deposit {
            builder.attached_deposit(deposit);
        }

        // Step 4: Return the completed context
        builder.build()
    }

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

    #[test]
    #[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
    fn test_delegate_fails_if_no_attached_deposit() {
        // Simulate a context where owner.near is calling the contract with 10 NEAR
        // and no attached deposit
        let context = get_context(owner(), NearToken::from_near(10), None);
        testing_env!(context);

        // Initialize a vault owned by owner.near
        let mut vault = Vault::new(owner(), 0, 1);

        // Attempt to call delegate method
        // This should panic due to assert_one_yocto
        vault.delegate(
            "validator.poolv1.near".parse().unwrap(),
            NearToken::from_yoctonear(1),
        );
    }

    #[test]
    #[should_panic(expected = "Only the vault owner can delegate stake")]
    fn test_delegate_fails_if_not_owner() {
        // alice tries to call delegate on a vault owned by owner.near
        let context = get_context(
            alice(),
            NearToken::from_near(10),
            Some(NearToken::from_yoctonear(1)),
        );
        testing_env!(context);

        // Initialize vault with owner.near as the owner
        let mut vault = Vault::new(owner(), 0, 1);

        // alice (not the owner) attempts to call delegate
        vault.delegate(
            "validator.poolv1.near".parse().unwrap(),
            NearToken::from_yoctonear(1),
        );
    }

    #[test]
    #[should_panic(expected = "Amount must be greater than 0")]
    fn test_delegate_fails_if_zero_amount() {
        // Set up context with correct owner, 10 NEAR balance, and 1 yoctoNEAR deposit
        let context = get_context(
            owner(),
            NearToken::from_near(10),
            Some(NearToken::from_yoctonear(1)),
        );
        testing_env!(context);

        // Initialize vault with owner.near
        let mut vault = Vault::new(owner(), 0, 1);

        // Attempt to delegate zero NEAR — should panic
        vault.delegate(
            "validator.poolv1.near".parse().unwrap(),
            NearToken::from_yoctonear(0),
        );
    }

    #[test]
    #[should_panic(expected = "Requested amount")]
    fn test_delegate_fails_if_insufficient_balance() {
        // Simulate the vault having exactly 1 NEAR total balance
        // STORAGE_BUFFER (0.01 NEAR) will be subtracted internally
        // So only 0.99 NEAR is available for delegation

        // Attach 1 yoctoNEAR to pass the assert_one_yocto check
        let context = get_context(
            owner(),
            NearToken::from_near(1),
            Some(NearToken::from_yoctonear(1)),
        );
        testing_env!(context);

        // Initialize the vault
        let mut vault = Vault::new(owner(), 0, 1);

        // Attempt to delegate 1 NEAR — this should panic
        // because get_available_balance will only allow 0.99 NEAR
        vault.delegate(
            "validator.poolv1.near".parse().unwrap(),
            NearToken::from_near(1),
        );
    }

    #[test]
    fn test_delegate_direct_executes_if_no_unstake_entries() {
        // Context: vault has 2 NEAR and attached 1 yoctoNEAR
        let context = get_context(
            owner(),
            NearToken::from_near(2),
            Some(NearToken::from_yoctonear(1)),
        );
        testing_env!(context);

        // Initialize vault owned by owner.near
        let mut vault = Vault::new(owner(), 0, 1);

        // Ensure the validator has no unstake entries
        assert!(vault
            .unstake_entries
            .get(&"validator.poolv1.near".parse().unwrap())
            .is_none());

        // Attempt to delegate 1 NEAR
        let _promise = vault.delegate(
            "validator.poolv1.near".parse().unwrap(),
            NearToken::from_near(1),
        );

        // Check validator is now tracked
        assert!(vault
            .active_validators
            .contains(&"validator.poolv1.near".parse().unwrap()));

        // Verify that the delegate_direct event was logged
        let logs = get_logs();
        let found_log = logs.iter().any(|log| log.contains("delegate_direct"));
        assert!(found_log, "Expected 'delegate_direct' log not found");
    }

    #[test]
    fn test_delegate_goes_through_withdraw_if_unstake_entries_exist() {
        // Setup test environment with:
        // - Contract account balance: 2 NEAR
        // - Attached deposit: 1 yoctoNEAR (required by assert_one_yocto)
        // - Caller: owner.near
        let context = get_context(
            owner(),
            NearToken::from_near(2),
            Some(NearToken::from_yoctonear(1)),
        );
        testing_env!(context);

        // The validator we will delegate to
        let validator: AccountId = "validator.poolv1.near".parse().unwrap();

        // Initialize a new vault owned by `owner.near`
        let mut vault = Vault::new(owner(), 0, 1);

        // Manually add a dummy unstake entry for the validator
        // This simulates the presence of unclaimed unbonded tokens
        let mut queue = Vector::new(StorageKey::UnstakeEntryPerValidator {
            validator_hash: env::sha256(validator.as_bytes()),
        });
        queue.push(&UnstakeEntry {
            amount: NearToken::from_near(1).as_yoctonear(),
            epoch_height: 100,
        });
        vault.unstake_entries.insert(&validator, &queue);

        // Call delegate with 1 NEAR
        // Because unstake_entries exist, the vault should go through:
        //   withdraw_all → reconcile → deposit_and_stake
        // NOT the fast path (delegate_direct)
        let _ = vault.delegate(validator.clone(), NearToken::from_near(1));

        // Inspect emitted logs
        // Should contain "delegate_started" but not "delegate_direct"
        let logs = get_logs();
        let found_delegate_direct = logs.iter().any(|log| log.contains("delegate_direct"));
        let found_delegate_started = logs.iter().any(|log| log.contains("delegate_started"));

        assert!(
            !found_delegate_direct,
            "Should not log 'delegate_direct' when unstake entries exist"
        );

        assert!(
            found_delegate_started,
            "Expected 'delegate_started' log not found"
        );
    }

    #[test]
    fn test_reconcile_unstake_entries_clears_fully_withdrawn_queue() {
        // Set up test context with owner and enough balance
        let context = get_context(owner(), NearToken::from_near(2), None);
        testing_env!(context);

        let validator: AccountId = "validator.poolv1.near".parse().unwrap();

        // Initialize vault
        let mut vault = Vault::new(owner(), 0, 1);

        // Add two unstake entries totaling 1 NEAR
        let mut queue = Vector::new(StorageKey::UnstakeEntryPerValidator {
            validator_hash: env::sha256(validator.as_bytes()),
        });
        queue.push(&UnstakeEntry {
            amount: 400_000_000_000_000_000_000_000,
            epoch_height: 100,
        });
        queue.push(&UnstakeEntry {
            amount: 600_000_000_000_000_000_000_000,
            epoch_height: 101,
        });
        vault.unstake_entries.insert(&validator, &queue);

        // Reconcile with full withdrawal of 1 NEAR
        vault.reconcile_unstake_entries(&validator, NearToken::from_near(1).as_yoctonear());

        // After reconciliation, both entries should be removed
        assert!(
            vault.unstake_entries.get(&validator).is_none(),
            "Unstake entry map should not contain validator"
        );
    }

    #[test]
    fn test_reconcile_unstake_entries_partial_removal() {
        // Set up test context with owner and enough balance
        let context = get_context(owner(), NearToken::from_near(2), None);
        testing_env!(context);

        let validator: AccountId = "validator.poolv1.near".parse().unwrap();

        // Initialize vault and add 2 unstake entries
        let mut vault = Vault::new(owner(), 0, 1);

        // Add two unstake entries totaling 1 NEAR
        let mut queue = Vector::new(StorageKey::UnstakeEntryPerValidator {
            validator_hash: env::sha256(validator.as_bytes()),
        });
        let entry_a = UnstakeEntry {
            amount: 400_000_000_000_000_000_000_000, // 0.4 NEAR
            epoch_height: 100,
        };
        let entry_b = UnstakeEntry {
            amount: 600_000_000_000_000_000_000_000, // 0.6 NEAR
            epoch_height: 101,
        };
        queue.push(&entry_a);
        queue.push(&entry_b);
        vault.unstake_entries.insert(&validator, &queue);

        // Simulate withdrawing only 0.4 NEAR
        vault.reconcile_unstake_entries(&validator, entry_a.amount);

        // Ensure:
        // - entry_a is removed
        // - entry_b is still present
        // - validator still tracked in unstaked_entries
        let new_queue = vault
            .unstake_entries
            .get(&validator)
            .expect("Queue should still exist");
        let remaining_entries: Vec<_> = new_queue.iter().collect();
        assert_eq!(remaining_entries.len(), 1, "Only one entry should remain");
        assert_eq!(
            remaining_entries[0].amount, entry_b.amount,
            "Remaining entry should match entry_b"
        );
    }

    #[test]
    fn test_reconcile_unstake_entries_handles_extra_rewards() {
        // Setup context with 2 NEAR balance
        let context = get_context(owner(), NearToken::from_near(2), None);
        testing_env!(context);

        let validator: AccountId = "validator.poolv1.near".parse().unwrap();

        // Initialize vault
        let mut vault = Vault::new(owner(), 0, 1);

        // Add one unstake entry for 1 NEAR
        let mut queue = Vector::new(StorageKey::UnstakeEntryPerValidator {
            validator_hash: env::sha256(validator.as_bytes()),
        });
        let entry = UnstakeEntry {
            amount: NearToken::from_near(1).as_yoctonear(),
            epoch_height: 100,
        };
        queue.push(&entry);
        vault.unstake_entries.insert(&validator, &queue);

        // Simulate total withdrawn = 1.5 NEAR (rewards included)
        vault.reconcile_unstake_entries(&validator, 1_500_000_000_000_000_000_000_000);

        // Validate that the unstake entry was removed
        assert!(
            vault.unstake_entries.get(&validator).is_none(),
            "Unstake entry should be cleared"
        );
    }

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
}
