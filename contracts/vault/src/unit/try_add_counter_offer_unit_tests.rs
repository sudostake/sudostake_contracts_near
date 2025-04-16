use near_sdk::{json_types::U128, testing_env, AccountId, NearToken};
use test_utils::{get_context, owner};

use crate::{
    contract::Vault,
    types::{CounterOfferMessage, LiquidityRequest},
};

#[path = "test_utils.rs"]
mod test_utils;

#[test]
fn test_adds_first_offer_successfully() {
    // Set up context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create a new vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Insert a liquidity request into the vault
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Create a valid counter offer message matching the request
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // Simulate a new offer from alice
    let proposer: AccountId = "alice.near".parse().unwrap();
    let result = vault.try_add_counter_offer(
        proposer.clone(),
        U128(800_000),
        msg,
        "usdc.test.near".parse().unwrap(),
    );

    // Assert it succeeded
    assert!(result.is_ok(), "Expected successful offer insertion");

    // Assert offer is stored
    let offer = vault
        .counter_offers
        .as_ref()
        .unwrap()
        .get(&proposer)
        .expect("Offer should exist");
    assert_eq!(offer.amount.0, 800_000);
}

#[test]
#[should_panic(expected = "Proposer already has an active offer")]
fn test_rejects_duplicate_proposer() {
    // Set up context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Create counter offer message
    let proposer: AccountId = "alice.near".parse().unwrap();
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // First offer should succeed
    let _ = vault.try_add_counter_offer(
        proposer.clone(),
        U128(800_000),
        msg.clone(),
        "usdc.test.near".parse().unwrap(),
    );

    // Second offer should panic
    vault
        .try_add_counter_offer(
            proposer,
            U128(750_000),
            msg,
            "usdc.test.near".parse().unwrap(),
        )
        .unwrap();
}

#[test]
#[should_panic(expected = "Offer must be less than requested amount")]
fn test_rejects_if_gte_requested_amount() {
    // Set up context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add liquidity request
    let requested_amount = U128(1_000_000);
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: requested_amount,
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Create counter offer message
    let proposer: AccountId = "alice.near".parse().unwrap();
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: "usdc.test.near".parse().unwrap(),
        amount: requested_amount,
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // Offer amount == requested amount — should panic
    vault
        .try_add_counter_offer(
            proposer,
            requested_amount,
            msg,
            "usdc.test.near".parse().unwrap(),
        )
        .unwrap();
}

#[test]
#[should_panic(expected = "Offer must be greater than current best offer")]
fn test_rejects_if_lte_best_offer() {
    // Set up context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add liquidity request
    let requested_amount = U128(1_000_000);
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: requested_amount,
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // First offer (800_000) — sets the best offer
    let proposer_1: AccountId = "bob.near".parse().unwrap();
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: "usdc.test.near".parse().unwrap(),
        amount: requested_amount,
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };
    vault
        .try_add_counter_offer(
            proposer_1.clone(),
            U128(800_000),
            msg.clone(),
            "usdc.test.near".parse().unwrap(),
        )
        .unwrap();

    // Second offer (700_000) — worse than best — should panic
    let proposer_2: AccountId = "carol.near".parse().unwrap();
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: "usdc.test.near".parse().unwrap(),
        amount: requested_amount,
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };
    vault
        .try_add_counter_offer(
            proposer_2,
            U128(700_000),
            msg,
            "usdc.test.near".parse().unwrap(),
        )
        .unwrap();
}

#[test]
#[should_panic(expected = "Message fields do not match current request")]
fn test_rejects_if_message_fields_mismatch() {
    // Set up context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add valid liquidity request
    let requested_amount = U128(1_000_000);
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: requested_amount,
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Create message with mismatched interest
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: "usdc.test.near".parse().unwrap(),
        amount: requested_amount,
        interest: U128(999_999), // ❌ MISMATCHED
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // This should panic due to mismatch
    let proposer: AccountId = "alice.near".parse().unwrap();
    vault
        .try_add_counter_offer(
            proposer,
            U128(900_000),
            msg,
            "usdc.test.near".parse().unwrap(),
        )
        .unwrap();
}

#[test]
fn test_eviction_happens_when_over_10_offers() {
    // Set up context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a liquidity request
    let requested_amount = U128(1_000_000);
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: requested_amount,
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Insert 10 offers with increasing amounts starting from 100_000
    for i in 0..10 {
        let proposer = format!("user_{}.near", i).parse().unwrap();
        let offered_amount = U128(100_000 + i * 10_000);

        let msg = CounterOfferMessage {
            action: "NewCounterOffer".to_string(),
            token: "usdc.test.near".parse().unwrap(),
            amount: requested_amount,
            interest: U128(100_000),
            collateral: NearToken::from_near(5),
            duration: 86400,
        };

        vault
            .try_add_counter_offer(
                proposer,
                offered_amount,
                msg,
                "usdc.test.near".parse().unwrap(),
            )
            .unwrap();
    }

    // At this point we have 10 entries
    assert_eq!(
        vault.counter_offers.as_ref().unwrap().len(),
        10,
        "Expected exactly 10 counter offers"
    );

    // Add the 11th offer with better amount (e.g. 999_000)
    let best_proposer: AccountId = "best.near".parse().unwrap();
    let best_amount = U128(999_000);
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: "usdc.test.near".parse().unwrap(),
        amount: requested_amount,
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };
    vault
        .try_add_counter_offer(
            best_proposer.clone(),
            best_amount,
            msg,
            "usdc.test.near".parse().unwrap(),
        )
        .unwrap();

    // Should still only have 10 offers
    let map = vault.counter_offers.as_ref().unwrap();
    assert_eq!(map.len(), 10, "Expected eviction to keep map size at 10");

    // Confirm best_proposer exists
    assert!(
        map.get(&best_proposer).is_some(),
        "Expected best_proposer to be in the map"
    );

    // Confirm worst offer was removed
    let evicted = "user_0.near".parse::<AccountId>().unwrap();
    assert!(
        map.get(&evicted).is_none(),
        "Expected user_0.near to be evicted"
    );
}

#[test]
#[should_panic(expected = "Token mismatch")]
fn test_rejects_if_token_does_not_match() {
    // Set up context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a liquidity request
    let requested_amount = U128(1_000_000);
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: requested_amount,
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Build a counter offer message matching the request
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: "usdc.token.near".parse().unwrap(),
        amount: requested_amount,
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // Simulate call coming from a different token contract (wrong origin)
    vault
        .try_add_counter_offer(
            "alice.near".parse().unwrap(),
            U128(900_000),
            msg,
            "fake.token.near".parse().unwrap(),
        )
        .unwrap();
}

#[test]
#[should_panic(expected = "No liquidity request available")]
fn test_rejects_if_no_liquidity_request_exists() {
    // Set up context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create a vault without setting liquidity_request
    let mut vault = Vault::new(owner(), 0, 1);

    // Try to add a counter offer
    let proposer: AccountId = "alice.near".parse().unwrap();
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(900_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // This should panic
    vault
        .try_add_counter_offer(
            proposer,
            U128(900_000),
            msg,
            "usdc.test.near".parse().unwrap(),
        )
        .unwrap();
}
