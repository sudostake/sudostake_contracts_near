use near_sdk::{json_types::U128, testing_env, NearToken, PromiseOrValue};
use serde_json::json;

use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;

use crate::contract::Vault;

use super::test_utils::{
    alice, bob, create_valid_liquidity_request, get_context, get_context_with_timestamp, owner,
};

#[test]
fn test_ft_on_transfer_refunds_on_invalid_message() {
    let token: near_sdk::AccountId = "usdc.mock.near".parse().unwrap();
    let context = get_context(token.clone(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    let amount = U128(123_456);

    let result = vault.ft_on_transfer(alice(), amount, "not valid json".to_string());

    match result {
        PromiseOrValue::Value(refunded) => assert_eq!(refunded, amount),
        PromiseOrValue::Promise(_) => panic!("Expected immediate refund for invalid message"),
    }
}

#[test]
fn test_ft_on_transfer_refunds_on_unknown_action() {
    let token: near_sdk::AccountId = "usdc.mock.near".parse().unwrap();
    let context = get_context(token.clone(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    let amount = U128(42);
    let sender = alice();

    let msg = json!({
        "action": "DoNothing",
        "token": token,
        "amount": amount,
        "interest": U128(1),
        "collateral": NearToken::from_near(1),
        "duration": 1
    })
    .to_string();

    let result = vault.ft_on_transfer(sender, amount, msg);

    match result {
        PromiseOrValue::Value(refunded) => assert_eq!(refunded, amount),
        PromiseOrValue::Promise(_) => panic!("Expected refund for unknown message action"),
    }
}

#[test]
fn test_ft_on_transfer_accepts_request_when_amount_matches() {
    let token: near_sdk::AccountId = "usdc.mock.near".parse().unwrap();
    let block_timestamp = 4242;
    let context = get_context_with_timestamp(
        token.clone(),
        NearToken::from_near(10),
        None,
        Some(block_timestamp),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    let request = create_valid_liquidity_request(token.clone());
    vault.liquidity_request = Some(request.clone());

    let msg = json!({
        "action": "ApplyCounterOffer",
        "token": request.token,
        "amount": request.amount,
        "interest": request.interest,
        "collateral": request.collateral,
        "duration": request.duration
    })
    .to_string();

    let lender = alice();
    let result = vault.ft_on_transfer(lender.clone(), request.amount, msg);

    match result {
        PromiseOrValue::Value(refunded) => assert_eq!(refunded, U128(0)),
        PromiseOrValue::Promise(_) => panic!("ft_on_transfer should not create promises"),
    }

    let accepted = vault
        .accepted_offer
        .expect("expected lender to be recorded as accepted");
    assert_eq!(
        accepted.lender, lender,
        "accepted lender should match sender"
    );
    assert_eq!(
        accepted.accepted_at, block_timestamp,
        "accepted timestamp should match block timestamp"
    );
    assert!(
        vault.counter_offers.is_none(),
        "counter offers should be cleared after acceptance"
    );
}

#[test]
fn test_ft_on_transfer_records_counter_offer_when_amount_is_lower() {
    let token: near_sdk::AccountId = "usdc.mock.near".parse().unwrap();
    let context = get_context(token.clone(), NearToken::from_near(10), None);
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    let request = create_valid_liquidity_request(token.clone());
    vault.liquidity_request = Some(request.clone());

    let msg = json!({
        "action": "ApplyCounterOffer",
        "token": request.token,
        "amount": request.amount,
        "interest": request.interest,
        "collateral": request.collateral,
        "duration": request.duration
    })
    .to_string();

    let lender = bob();
    let offer_amount = U128(request.amount.0 - 100_000);
    let result = vault.ft_on_transfer(lender.clone(), offer_amount, msg);

    match result {
        PromiseOrValue::Value(refunded) => assert_eq!(refunded, U128(0)),
        PromiseOrValue::Promise(_) => panic!("ft_on_transfer should not create promises"),
    }

    assert!(
        vault.accepted_offer.is_none(),
        "lender should not be auto-accepted for partial amount"
    );

    let offers = vault
        .counter_offers
        .as_ref()
        .expect("counter offer map should exist");
    let offer = offers
        .get(&lender)
        .expect("counter offer should be recorded for lender");
    assert_eq!(
        offer.amount, offer_amount,
        "stored counter offer amount should match attached amount"
    );
}
