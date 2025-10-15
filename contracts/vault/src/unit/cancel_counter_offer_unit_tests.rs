use near_sdk::{collections::UnorderedMap, json_types::U128, testing_env, AccountId, NearToken};
use test_utils::{get_context, owner};

use crate::{
    contract::Vault,
    types::{AcceptedOffer, ApplyCounterOfferMessage, LiquidityRequest, StorageKey},
    unit::test_utils::alice,
};

#[path = "test_utils.rs"]
mod test_utils;

#[test]
fn test_cancel_counter_offer_succeeds() {
    // Set up test context as alice
    let ctx = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
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
    let msg = ApplyCounterOfferMessage {
        action: "ApplyCounterOffer".to_string(),
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // Simulate a new offer from alice
    let proposer: AccountId = alice();
    vault
        .try_add_counter_offer(
            proposer.clone(),
            U128(800_000),
            msg,
            "usdc.test.near".parse().unwrap(),
        )
        .expect("counter offer should be recorded");

    // Call cancel_counter_offer as alice
    vault.cancel_counter_offer();

    // Verify state changes
    assert!(
        vault.counter_offers.is_none(),
        "Counter offers map should be cleared after cancellation"
    );
}

#[test]
fn test_cancel_counter_offer_clears_underlying_storage_when_last_offer() {
    // Set up test context as alice
    let ctx = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    let mut vault = Vault::new(owner(), 0, 1);

    let token: AccountId = "usdc.test.near".parse().unwrap();
    vault.liquidity_request = Some(LiquidityRequest {
        token: token.clone(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    let msg = ApplyCounterOfferMessage {
        action: "ApplyCounterOffer".to_string(),
        token: token.clone(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    vault
        .try_add_counter_offer(alice(), U128(800_000), msg, token)
        .expect("offer creation should succeed");

    vault.cancel_counter_offer();

    let inspector: UnorderedMap<AccountId, crate::types::CounterOffer> =
        UnorderedMap::new(StorageKey::CounterOffers);
    assert_eq!(
        inspector.len(),
        0,
        "Counter offer storage prefix should be empty after final cancel"
    );
}

#[test]
#[should_panic(expected = "No liquidity request open")]
fn test_cancel_fails_if_no_liquidity_request() {
    // Set up test context as alice
    let ctx = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create a new vault
    let mut vault = Vault::new(owner(), 0, 1);

    // try to cancel offer
    vault.cancel_counter_offer();
}

#[test]
#[should_panic(expected = "Cannot cancel after offer is accepted")]
fn test_cancel_fails_if_offer_already_accepted() {
    // Set up test context as alice
    let ctx = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
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

    // Simulate an already accepted liquidity request
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at: 12345678,
    });

    // try to cancel offer
    vault.cancel_counter_offer();
}

#[test]
#[should_panic(expected = "No counter offers found")]
fn test_cancel_fails_if_no_offer_exists() {
    // Set up test context as alice
    let ctx = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
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

    // try to cancel offer
    vault.cancel_counter_offer();
}

#[test]
#[should_panic(expected = "No active offer to cancel")]
fn test_cancel_fails_if_offer_not_from_caller() {
    // Set up context as alice (who did NOT place the offer)
    let ctx = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
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

    // Create a valid counter offer message matching the request
    let msg = ApplyCounterOfferMessage {
        action: "ApplyCounterOffer".to_string(),
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // Add a counter offer from bob (not alice)
    vault
        .try_add_counter_offer(
            "bob.near".parse().unwrap(),
            U128(800_000),
            msg,
            "usdc.test.near".parse().unwrap(),
        )
        .unwrap();

    // Alice attempts to cancel (should panic)
    vault.cancel_counter_offer();
}
