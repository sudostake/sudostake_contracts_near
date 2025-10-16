use near_sdk::{
    collections::UnorderedMap, json_types::U128, test_utils::get_logs, testing_env, AccountId,
    NearToken,
};
use test_utils::{
    apply_counter_offer_message_from, create_valid_liquidity_request, get_context, owner,
};

use crate::{
    contract::Vault,
    types::{AcceptedOffer, StorageKey},
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
    let token: AccountId = "usdc.test.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());
    vault.liquidity_request = Some(request.clone());

    // Create a valid counter offer message matching the request
    let msg = apply_counter_offer_message_from(&request);

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
fn test_cancel_counter_offer_emits_event() {
    let ctx = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(ctx);

    let mut vault = Vault::new(owner(), 0, 1);

    let token: AccountId = "usdc.test.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());
    vault.liquidity_request = Some(request.clone());

    let msg = apply_counter_offer_message_from(&request);

    vault
        .try_add_counter_offer(alice(), U128(800_000), msg, token)
        .expect("offer creation should succeed");

    vault.cancel_counter_offer();

    let logs = get_logs();
    let event_log = logs
        .iter()
        .rev()
        .find(|log| log.contains("counter_offer_cancelled"))
        .expect("Expected counter_offer_cancelled log entry");

    let payload = event_log
        .strip_prefix("EVENT_JSON:")
        .expect("Log entry should start with EVENT_JSON:");
    let payload: serde_json::Value =
        serde_json::from_str(payload).expect("Log entry should be valid JSON");

    assert_eq!(
        payload.get("event").and_then(|v| v.as_str()),
        Some("counter_offer_cancelled")
    );

    let data = payload
        .get("data")
        .expect("counter_offer_cancelled log missing data field");
    assert!(
        data.get("vault")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "vault field should be a non-empty string"
    );

    let proposer = alice().to_string();
    assert_eq!(
        data.get("proposer").and_then(|v| v.as_str()),
        Some(proposer.as_str())
    );
    assert_eq!(
        data.get("amount").and_then(|v| v.as_str()),
        Some("800000")
    );

    let request_data = data
        .get("request")
        .and_then(|v| v.as_object())
        .expect("counter_offer_cancelled log missing request payload");
    assert_eq!(
        request_data.get("token").and_then(|v| v.as_str()),
        Some("usdc.test.near")
    );
    assert_eq!(
        request_data.get("amount").and_then(|v| v.as_str()),
        Some("1000000")
    );
    assert_eq!(
        request_data.get("interest").and_then(|v| v.as_str()),
        Some("100000")
    );

    let expected_collateral = NearToken::from_near(5)
        .as_yoctonear()
        .to_string();
    assert_eq!(
        request_data
            .get("collateral")
            .and_then(|v| v.as_str()),
        Some(expected_collateral.as_str())
    );
    assert_eq!(
        request_data.get("duration").and_then(|v| v.as_u64()),
        Some(86400)
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
    let request = create_valid_liquidity_request(token.clone());
    vault.liquidity_request = Some(request.clone());

    let msg = apply_counter_offer_message_from(&request);

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
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_cancel_counter_offer_requires_one_yocto() {
    let ctx = get_context(alice(), NearToken::from_near(10), None);
    testing_env!(ctx);

    let mut vault = Vault::new(owner(), 0, 1);
    vault.cancel_counter_offer();
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
    let token: AccountId = "usdc.test.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());
    vault.liquidity_request = Some(request.clone());

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
    let token: AccountId = "usdc.test.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());
    vault.liquidity_request = Some(request.clone());

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
    let token: AccountId = "usdc.test.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());
    vault.liquidity_request = Some(request.clone());

    // Create a valid counter offer message matching the request
    let msg = apply_counter_offer_message_from(&request);

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
