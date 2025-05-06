#[path = "test_utils.rs"]
mod test_utils;

use crate::{
    contract::Vault,
    types::{AcceptedOffer, CounterOffer, LiquidityRequest, PendingLiquidityRequest, StorageKey},
};
use near_sdk::{collections::UnorderedMap, json_types::U128, testing_env, AccountId, NearToken};
use test_utils::{alice, get_context, owner};

#[test]
fn test_request_liquidity_success() {
    // Setup context
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add one active validator
    vault
        .active_validators
        .insert(&"validator1.near".parse::<AccountId>().unwrap());

    // Call request_liquidity
    vault.request_liquidity(
        "usdc.token.near".parse().unwrap(),
        U128(1_000_000),
        U128(100_000),
        NearToken::from_near(5),
        60 * 60 * 24,
    );

    // Assert vault state
    assert!(vault.pending_liquidity_request.is_some());
    assert!(vault.liquidity_request.is_none());
    assert!(vault.counter_offers.is_none());
}

#[test]
#[should_panic(expected = "Only the vault owner can request liquidity")]
fn test_request_liquidity_fails_if_not_owner() {
    // Setup context
    let ctx = get_context(
        alice(), // Not the vault owner
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault with correct owner
    let mut vault = Vault::new(owner(), 0, 1);

    // Add at least one validator
    vault
        .active_validators
        .insert(&"validator1.near".parse::<AccountId>().unwrap());

    // Attempt to request liquidity as non-owner (should panic)
    vault.request_liquidity(
        "usdc.token.near".parse().unwrap(),
        U128(1_000_000),
        U128(100_000),
        NearToken::from_near(5),
        60 * 60 * 24,
    );
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_request_liquidity_fails_if_deposit_not_1_yocto() {
    // Setup context with incorrect attached deposit (e.g., 0)
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(0)),
    );
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add one validator
    vault
        .active_validators
        .insert(&"validator1.near".parse::<AccountId>().unwrap());

    // Should panic due to invalid deposit
    vault.request_liquidity(
        "usdc.token.near".parse().unwrap(),
        U128(1_000_000),
        U128(100_000),
        NearToken::from_near(5),
        60 * 60 * 24,
    );
}

#[test]
#[should_panic(expected = "A liquidity request is already in progress")]
fn test_request_liquidity_fails_if_pending_request_exists() {
    // Setup context
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add validator
    vault
        .active_validators
        .insert(&"validator1.near".parse::<AccountId>().unwrap());

    // Manually insert a pending request
    vault.pending_liquidity_request = Some(PendingLiquidityRequest {
        token: "usdc.token.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 60 * 60 * 24,
    });

    // Attempting to call request_liquidity again should panic
    vault.request_liquidity(
        "usdc.token.near".parse().unwrap(),
        U128(1_000_000),
        U128(100_000),
        NearToken::from_near(5),
        60 * 60 * 24,
    );
}

#[test]
#[should_panic(expected = "A request is already open")]
fn test_request_liquidity_fails_if_liquidity_request_exists() {
    // Setup context
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a validator
    vault
        .active_validators
        .insert(&"validator1.near".parse().unwrap());

    // Simulate an existing liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.token.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 60 * 60 * 24,
        created_at: 0,
    });

    // Should panic due to existing liquidity request
    let _ = vault.request_liquidity(
        "usdc.token.near".parse().unwrap(),
        U128(1_000_000),
        U128(100_000),
        NearToken::from_near(5),
        60 * 60 * 24,
    );
}

#[test]
#[should_panic(expected = "Vault is already matched with a lender")]
fn test_request_liquidity_fails_if_accepted_offer_exists() {
    // Setup context
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add validator
    vault
        .active_validators
        .insert(&"validator1.near".parse().unwrap());

    // Simulate an accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.near".parse().unwrap(),
        accepted_at: 0,
    });

    // Should panic due to existing accepted offer
    vault.request_liquidity(
        "usdc.token.near".parse().unwrap(),
        U128(1_000_000),
        U128(100_000),
        NearToken::from_near(5),
        60 * 60 * 24,
    );
}

#[test]
#[should_panic(expected = "Counter-offers must be cleared")]
fn test_request_liquidity_fails_if_counter_offers_exists() {
    // Setup context
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add validator
    vault
        .active_validators
        .insert(&"validator1.near".parse().unwrap());

    // Simulate existing counter offers
    let mut offers = UnorderedMap::new(StorageKey::CounterOffers);
    offers.insert(
        &"lender.near".parse().unwrap(),
        &CounterOffer {
            proposer: "lender.near".parse().unwrap(),
            amount: U128(900_000),
            timestamp: 0,
        },
    );
    vault.counter_offers = Some(offers);

    // Should panic due to lingering counter offers
    vault.request_liquidity(
        "usdc.token.near".parse().unwrap(),
        U128(1_000_000),
        U128(100_000),
        NearToken::from_near(5),
        60 * 60 * 24,
    );
}

#[test]
#[should_panic(expected = "Collateral must be positive")]
fn test_request_liquidity_fails_if_collateral_is_zero() {
    // Setup context
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add validator
    vault
        .active_validators
        .insert(&"validator1.near".parse().unwrap());

    // Try request_liquidity with 0 NEAR collateral → should panic
    vault.request_liquidity(
        "usdc.token.near".parse().unwrap(),
        U128(1_000_000),
        U128(100_000),
        NearToken::from_near(0), // invalid collateral
        60 * 60 * 24,
    );
}

#[test]
#[should_panic(expected = "Requested amount must be greater than zero")]
fn test_request_liquidity_fails_if_amount_is_zero() {
    // Setup context
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add validator
    vault
        .active_validators
        .insert(&"validator1.near".parse().unwrap());

    // Call with amount = 0
    let _ = vault.request_liquidity(
        "usdc.token.near".parse().unwrap(),
        U128(0), // invalid amount
        U128(100_000),
        NearToken::from_near(5),
        60 * 60 * 24,
    );
}

#[test]
#[should_panic(expected = "Duration must be non-zero")]
fn test_request_liquidity_fails_if_duration_is_zero() {
    // Setup context
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add validator
    vault
        .active_validators
        .insert(&"validator1.near".parse().unwrap());

    // Attempt request with duration = 0
    vault.request_liquidity(
        "usdc.token.near".parse().unwrap(),
        U128(1_000_000),
        U128(100_000),
        NearToken::from_near(5),
        0, // invalid duration
    );
}

#[test]
#[should_panic(expected = "Expected a pending liquidity request")]
fn test_on_check_total_staked_fails_if_no_pending_request() {
    // Setup context
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Vault without pending_liquidity_request
    let mut vault = Vault::new(owner(), 0, 1);

    // Call on_check_total_staked → should panic
    vault.on_check_total_staked();
}

#[test]
#[should_panic(expected = "Vault busy with RequestLiquidity")]
fn test_request_liquidity_fails_if_vault_already_locked() {
    // Simulate a timestamp of 1_000_000_000_000_000_000 (e.g. 1s)
    let locked_at = 1_000_000_000_000_000_000;

    // Set current time shortly after lock timestamp (still within timeout)
    let now = locked_at + 10_000_000_000; // +0.01s

    // Setup context with vault owner and 1 yoctoNEAR attached
    let ctx = test_utils::get_context_with_timestamp(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
        Some(now),
    );
    testing_env!(ctx);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a validator
    vault
        .active_validators
        .insert(&"validator1.near".parse().unwrap());

    // Simulate a stale-inactive lock is not yet expired
    vault.processing_state = crate::types::ProcessingState::RequestLiquidity;
    vault.processing_since = locked_at;

    // Attempt to request liquidity while lock is still valid → should panic
    vault.request_liquidity(
        "usdc.token.near".parse().unwrap(),
        U128(1_000_000),
        U128(100_000),
        NearToken::from_near(5),
        60 * 60 * 24,
    );
}
