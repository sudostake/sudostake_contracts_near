use near_sdk::{json_types::U128, testing_env, AccountId, NearToken};
use test_utils::{get_context, owner};

use crate::{
    contract::Vault,
    types::{AcceptedOffer, CounterOfferMessage, LiquidityRequest},
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
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // Simulate a new offer from alice
    let proposer: AccountId = alice();
    let _ = vault.try_add_counter_offer(
        proposer.clone(),
        U128(800_000),
        msg,
        "usdc.test.near".parse().unwrap(),
    );

    // Call cancel_counter_offer as alice
    vault.cancel_counter_offer();

    // Verify state changes
    assert!(
        vault.counter_offers.is_none(),
        "Counter offers map should be cleared after cancellation"
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
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
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
