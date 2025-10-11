use near_sdk::{collections::UnorderedMap, json_types::U128, testing_env, AccountId, NearToken};
use test_utils::{create_valid_liquidity_request, get_context, owner};

use crate::{
    contract::Vault,
    types::{AcceptedOffer, CounterOffer, StorageKey},
    unit::test_utils::alice,
};

#[path = "test_utils.rs"]
mod test_utils;

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_cancel_liquidity_request_fails_if_missing_yocto() {
    // Setup context with 0 attached deposit
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(0)),
    );
    testing_env!(ctx);

    // Create vault with a liquidity request
    let mut vault = Vault::new(owner(), 0, 1);
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.token.near".parse().unwrap(),
    ));

    // Should panic due to missing 1 yoctoNEAR
    vault.cancel_liquidity_request();
}

#[test]
#[should_panic(expected = "Only the vault owner can cancel the liquidity request")]
fn test_cancel_liquidity_request_fails_if_not_owner() {
    // Setup context where alice (not the owner) tries to cancel
    let ctx = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault owned by "owner"
    let mut vault = Vault::new(owner(), 0, 1);
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.token.near".parse().unwrap(),
    ));

    // Should panic due to unauthorized caller
    vault.cancel_liquidity_request();
}

#[test]
#[should_panic(expected = "No active liquidity request")]
fn test_cancel_liquidity_request_fails_if_no_request() {
    // Setup context as owner with correct deposit
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create vault with no liquidity request
    let mut vault = Vault::new(owner(), 0, 1);

    // Should panic due to no active liquidity request
    vault.cancel_liquidity_request();
}

#[test]
#[should_panic(expected = "Cannot cancel after an offer has been accepted")]
fn test_cancel_liquidity_request_fails_if_offer_accepted() {
    // Setup context as vault owner with 1 yoctoNEAR attached
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create a vault owned by the caller
    let mut vault = Vault::new(owner(), 0, 1);

    // Attach a valid liquidity request
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.token.near".parse().unwrap(),
    ));

    // Simulate that an offer has already been accepted
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at: 0,
    });

    // Call cancel_liquidity_request — should panic
    vault.cancel_liquidity_request();
}

#[test]
fn test_cancel_liquidity_request_succeeds_and_clears_state() {
    // Setup context as owner with correct deposit
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create a vault owned by the caller
    let mut vault = Vault::new(owner(), 0, 1);

    // Attach a valid liquidity request
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.token.near".parse().unwrap(),
    ));

    // Add a sample counter offer from Alice
    let mut counter_offers = UnorderedMap::new(StorageKey::CounterOffers);
    counter_offers.insert(
        &alice(),
        &CounterOffer {
            proposer: alice(),
            amount: U128(1_000_000),
            timestamp: 0,
        },
    );
    vault.counter_offers = Some(counter_offers);

    // Call cancel_liquidity_request as the owner
    vault.cancel_liquidity_request();

    // Assert that liquidity request was cleared
    assert!(
        vault.liquidity_request.is_none(),
        "Liquidity request should be cleared"
    );

    // Assert that counter offers were also cleared
    assert!(
        vault.counter_offers.is_none(),
        "Counter offers should be cleared"
    );
}

#[test]
fn test_cancel_liquidity_request_clears_underlying_storage() {
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    let mut vault = Vault::new(owner(), 0, 1);
    let token = "usdc.token.near".parse().unwrap();
    vault.liquidity_request = Some(create_valid_liquidity_request(token));

    let mut counter_offers = UnorderedMap::new(StorageKey::CounterOffers);
    counter_offers.insert(
        &alice(),
        &CounterOffer {
            proposer: alice(),
            amount: U128(1_000_000),
            timestamp: 0,
        },
    );
    vault.counter_offers = Some(counter_offers);

    vault.cancel_liquidity_request();

    let inspector: UnorderedMap<AccountId, CounterOffer> =
        UnorderedMap::new(StorageKey::CounterOffers);
    assert_eq!(
        inspector.len(),
        0,
        "Counter offer storage should be cleared"
    );
}

#[test]
fn test_cancel_liquidity_request_succeeds_with_empty_counter_offers() {
    // Setup context as vault owner with 1 yoctoNEAR attached
    let ctx = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    // Create a vault owned by the caller
    let mut vault = Vault::new(owner(), 0, 1);

    // Attach a valid liquidity request
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.token.near".parse().unwrap(),
    ));

    // Add an empty counter_offers map
    vault.counter_offers = Some(UnorderedMap::new(StorageKey::CounterOffers));

    // Call cancel_liquidity_request — should succeed
    vault.cancel_liquidity_request();

    // Assert state is cleared
    assert!(
        vault.liquidity_request.is_none(),
        "Liquidity request should be cleared"
    );
    assert!(
        vault.counter_offers.is_none(),
        "Counter offers should be cleared"
    );
}
