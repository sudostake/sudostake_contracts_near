#[cfg(test)]
mod tests {
    use near_sdk::{json_types::U128, testing_env, AccountId, NearToken};

    use crate::{
        contract::Vault,
        types::{AcceptedOffer, CounterOffer, Liquidation, RefundEntry, StorageKey, UnstakeEntry},
    };

    use super::super::test_utils::{alice, bob, get_context, insert_refund_entry, owner};

    use near_sdk::collections::{UnorderedMap, UnorderedSet};

    #[test]
    fn test_get_vault_state_reflects_core_fields() {
        let context = get_context(owner(), NearToken::from_near(10), None);
        testing_env!(context);

        let mut vault = Vault::new(owner(), 1, 42);

        vault.liquidity_request = Some(super::super::test_utils::create_valid_liquidity_request(
            "usdc.test.near".parse().unwrap(),
        ));
        vault.accepted_offer = Some(AcceptedOffer {
            lender: alice(),
            accepted_at: 777,
        });
        vault.is_listed_for_takeover = true;
        vault.liquidation = Some(Liquidation {
            liquidated: NearToken::from_near(1),
        });

        let mut validators = UnorderedSet::new(StorageKey::ActiveValidators);
        validators.insert(&"validator.test.near".parse::<AccountId>().unwrap());
        vault.active_validators = validators;

        let mut unstake_entries = UnorderedMap::new(StorageKey::UnstakeEntries);
        unstake_entries.insert(
            &"validator.test.near".parse().unwrap(),
            &UnstakeEntry {
                amount: 500,
                epoch_height: 100,
            },
        );
        vault.unstake_entries = unstake_entries;

        let state = vault.get_vault_state();
        assert_eq!(state.owner, owner());
        assert!(state.liquidity_request.is_some());
        assert!(state.accepted_offer.is_some());
        assert!(state.is_listed_for_takeover);
        assert_eq!(state.active_validators.len(), 1);
        assert_eq!(state.unstake_entries.len(), 1);
        assert!(state.liquidation.is_some());
    }

    #[test]
    fn test_get_active_validators_and_unstake_entry() {
        let context = get_context(owner(), NearToken::from_near(10), None);
        testing_env!(context);

        let mut vault = Vault::new(owner(), 0, 1);

        vault
            .active_validators
            .insert(&"validator.test.near".parse().unwrap());
        vault.unstake_entries.insert(
            &"validator.test.near".parse().unwrap(),
            &UnstakeEntry {
                amount: 1_000,
                epoch_height: 5,
            },
        );

        let validators = vault.get_active_validators();
        assert_eq!(validators, vec!["validator.test.near".to_string()]);

        let unstake_entry = vault
            .get_unstake_entry("validator.test.near".parse().unwrap())
            .expect("expected unstake entry");
        assert_eq!(unstake_entry.amount, 1_000);
        assert_eq!(unstake_entry.epoch_height, 5);
    }

    #[test]
    fn test_get_counter_offers_serialises_map() {
        let context = get_context(owner(), NearToken::from_near(10), None);
        testing_env!(context);

        let mut vault = Vault::new(owner(), 0, 1);
        let mut offers = UnorderedMap::new(StorageKey::CounterOffers);
        offers.insert(
            &alice(),
            &CounterOffer {
                proposer: alice(),
                amount: U128(500_000),
                timestamp: 123,
            },
        );
        vault.counter_offers = Some(offers);

        let counter_offers = vault
            .get_counter_offers()
            .expect("expected counter offers to be returned");
        assert!(counter_offers.contains_key(&alice()));
        assert_eq!(counter_offers[&alice()].amount.0, 500_000);
    }

    #[test]
    fn test_view_available_balance_and_storage_cost() {
        let mut context = near_sdk::test_utils::VMContextBuilder::new();
        context
            .predecessor_account_id(owner())
            .account_balance(NearToken::from_yoctonear(5 * 10u128.pow(24)))
            .storage_usage(1_000);
        testing_env!(context.build());

        let vault = Vault::new(owner(), 0, 1);

        let storage_cost = vault.get_storage_cost();
        let view_storage_cost: u128 = vault.view_storage_cost().into();
        assert_eq!(storage_cost, view_storage_cost);

        let available = vault.view_available_balance();
        let expected = vault.get_available_balance().as_yoctonear();
        assert_eq!(available.0, expected);
    }

    #[test]
    fn test_get_refund_entries_filters_by_account() {
        let context = get_context(owner(), NearToken::from_near(10), None);
        testing_env!(context);

        let mut vault = Vault::new(owner(), 0, 1);
        insert_refund_entry(
            &mut vault,
            1,
            RefundEntry {
                token: None,
                proposer: alice(),
                amount: U128(100),
                added_at_epoch: 1,
            },
        );
        insert_refund_entry(
            &mut vault,
            2,
            RefundEntry {
                token: None,
                proposer: bob(),
                amount: U128(200),
                added_at_epoch: 1,
            },
        );

        let all_entries = vault.get_refund_entries(None);
        assert_eq!(all_entries.len(), 2);

        let alice_entries = vault.get_refund_entries(Some(alice()));
        assert_eq!(alice_entries.len(), 1);
        assert_eq!(alice_entries[0].1.amount.0, 100);
    }
}
