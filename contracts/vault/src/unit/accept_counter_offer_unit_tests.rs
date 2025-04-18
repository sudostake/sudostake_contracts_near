use near_sdk::{json_types::U128, testing_env, AccountId, NearToken};
use test_utils::{alice, get_context, owner};

use crate::{
    contract::Vault,
    types::{AcceptedOffer, CounterOfferMessage, LiquidityRequest},
};

#[path = "test_utils.rs"]
mod test_utils;

#[test]
fn test_accept_counter_offer_succeeds() {
    // Set context as vault owner
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Initialize vault and add liquidity request
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

    // Construct a valid counter offer message
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: token.clone(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // Propose counter offer by bob
    let _ = vault.try_add_counter_offer(
        "bob.near".parse().unwrap(),
        U128(850_000),
        msg.clone(),
        token.clone(),
    );

    // Propose counter offer by alice
    let _ = vault.try_add_counter_offer(alice(), U128(900_000), msg.clone(), token.clone());

    // Accept alice's offer
    vault.accept_counter_offer(alice(), U128(900_000));

    // Assert vault state updated
    let accepted = vault
        .accepted_offer
        .as_ref()
        .expect("Expected accepted offer");
    assert_eq!(accepted.lender, alice(), "Accepted lender should be alice");

    // Assert counter offers cleared
    assert!(
        vault.counter_offers.is_none(),
        "Counter offers should be cleared after acceptance"
    );
}

#[test]
#[should_panic(expected = "Only the vault owner can accept a counter offer")]
fn test_accept_fails_if_not_owner() {
    // Set context as alice (not the owner)
    let ctx = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Initialize vault and add liquidity request
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

    // Add a valid counter offer from alice
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: token.clone(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };
    vault
        .try_add_counter_offer(alice(), U128(900_000), msg, token.clone())
        .expect("Offer should be added successfully");

    // Alice attempts to accept her own offer (should panic)
    vault.accept_counter_offer(alice(), U128(900_000));
}

#[test]
#[should_panic(expected = "No liquidity request available")]
fn test_accept_fails_if_no_liquidity_request() {
    // Set context as vault owner
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Initialize vault without liquidity request
    let mut vault = Vault::new(owner(), 0, 1);

    // Try to accept (should panic because liquidity_request is None)
    vault.accept_counter_offer(alice(), U128(900_000));
}

#[test]
#[should_panic(expected = "Liquidity request already accepted")]
fn test_accept_fails_if_already_accepted() {
    // Set context as vault owner
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a liquidity request
    let token: AccountId = "usdc.test.near".parse().unwrap();
    vault.liquidity_request = Some(LiquidityRequest {
        token: token.clone(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Add an already accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "bob.near".parse().unwrap(),
        accepted_at: 12345678,
    });

    // Attempt to accept another offer (should panic)
    vault.accept_counter_offer(alice(), U128(900_000));
}

#[test]
#[should_panic(expected = "No counter offers available")]
fn test_accept_fails_if_counter_offers_empty() {
    // Set context as vault owner
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create a vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Call accept_counter_offer without adding any offers
    vault.accept_counter_offer(alice(), U128(900_000));
}

#[test]
#[should_panic(expected = "Counter offer from proposer not found")]
fn test_accept_fails_if_proposer_does_not_exist() {
    // Set context as vault owner
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create a vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Create counter offer message
    let token: AccountId = "usdc.test.near".parse().unwrap();
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: token.clone(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // Add counter offer from bob
    vault
        .try_add_counter_offer(
            "bob.near".parse().unwrap(),
            U128(850_000),
            msg,
            token.clone(),
        )
        .expect("bob's offer should succeed");

    // Attempt to accept alice's offer which doesn't exist
    vault.accept_counter_offer(alice(), U128(900_000));
}

#[test]
#[should_panic(expected = "Provided amount does not match the counter offer")]
fn test_accept_fails_if_amount_mismatch() {
    // Set context as vault owner
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create a vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Create counter offer message
    let token: AccountId = "usdc.test.near".parse().unwrap();
    let msg = CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: token.clone(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    };

    // Add a counter offer by alice
    vault
        .try_add_counter_offer(alice(), U128(900_000), msg, token.clone())
        .expect("alice's offer should succeed");

    // Accept alice's offer but pass the wrong amount (800_000 instead of 900_000)
    vault.accept_counter_offer(alice(), U128(800_000));
}
