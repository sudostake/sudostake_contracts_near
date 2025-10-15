use crate::{
    contract::Vault,
    types::{CounterOffer, StorageKey},
};
use near_sdk::{collections::UnorderedMap, json_types::U128, testing_env, AccountId, NearToken};
use test_utils::{
    alice, apply_counter_offer_message_from, bob, create_valid_liquidity_request, get_context,
    owner,
};

#[path = "test_utils.rs"]
mod test_utils;

#[test]
fn test_try_accept_liquidity_request_success() {
    // Setup context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create token account ID
    let token: AccountId = "usdc.token.near".parse().unwrap();

    // Create a valid liquidity request
    let request = create_valid_liquidity_request(token.clone());

    // Initialize the vault with the liquidity request
    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());

    // Construct the message the lender will send
    let msg = apply_counter_offer_message_from(&request);

    // Define lender and token contract
    let lender = alice();
    let token_contract = token.clone();

    // Attempt to accept the liquidity request
    let result =
        contract.try_accept_liquidity_request(lender.clone(), request.amount, msg, token_contract);

    // Expect success
    assert!(result.is_ok(), "Expected success, got: {:?}", result);

    // Verify accepted_offer is correctly set
    let accepted = contract
        .accepted_offer
        .expect("Expected accepted_offer to be set");
    assert_eq!(accepted.lender, lender);
}

#[test]
fn test_try_accept_liquidity_request_clears_counter_offers() {
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    let token: AccountId = "usdc.token.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());

    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());

    let mut map = UnorderedMap::new(StorageKey::CounterOffers);
    map.insert(
        &alice(),
        &CounterOffer {
            proposer: alice(),
            amount: U128(900_000),
            timestamp: 42,
        },
    );
    contract.counter_offers = Some(map);

    let msg = apply_counter_offer_message_from(&request);

    let lender = bob();
    let result = contract.try_accept_liquidity_request(lender, request.amount, msg, token);

    assert!(result.is_ok(), "Expected success, got: {:?}", result);
    assert!(
        contract.counter_offers.is_none(),
        "Counter offers were not cleared"
    );
}

#[test]
fn test_try_accept_liquidity_request_clears_underlying_storage() {
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    let token: AccountId = "usdc.token.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());

    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());

    let mut map = UnorderedMap::new(StorageKey::CounterOffers);
    map.insert(
        &alice(),
        &CounterOffer {
            proposer: alice(),
            amount: U128(900_000),
            timestamp: 42,
        },
    );
    contract.counter_offers = Some(map);

    let msg = apply_counter_offer_message_from(&request);

    let lender = bob();
    let result = contract.try_accept_liquidity_request(lender, request.amount, msg, token);

    assert!(result.is_ok(), "Expected success, got: {:?}", result);

    // Recreate the map with the same storage prefix to verify on-chain cleanup
    let inspector: UnorderedMap<AccountId, CounterOffer> =
        UnorderedMap::new(StorageKey::CounterOffers);
    assert_eq!(
        inspector.len(),
        0,
        "Counter offer storage prefix should be cleared"
    );
}

#[test]
fn test_try_accept_liquidity_request_fails_if_no_request() {
    // Setup context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create token account ID
    let token: AccountId = "usdc.token.near".parse().unwrap();

    // Initialize vault WITHOUT any liquidity_request
    let mut contract = Vault::new(owner(), 0, 1);

    // Construct a message that would match a valid request
    let request = create_valid_liquidity_request(token.clone());
    let msg = apply_counter_offer_message_from(&request);

    // Attempt to accept the request
    let result = contract.try_accept_liquidity_request(alice(), U128(1_000_000), msg, token);

    // Expect an error due to missing liquidity_request
    assert!(result.is_err(), "Expected failure but got success");
    assert_eq!(result.unwrap_err(), "No liquidity request available");
}

#[test]
fn test_try_accept_liquidity_request_fails_if_already_accepted() {
    // Setup context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create token account ID
    let token: AccountId = "usdc.token.near".parse().unwrap();

    // Create liquidity request
    let request = create_valid_liquidity_request(token.clone());

    // Initialize vault with liquidity request and accepted_offer already set
    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());
    contract.accepted_offer = Some(crate::types::AcceptedOffer {
        lender: alice(),
        accepted_at: 12345678,
    });

    // Construct a matching accept message
    let msg = apply_counter_offer_message_from(&request);

    // Attempt to accept again
    let result = contract.try_accept_liquidity_request(alice(), request.amount, msg, token);

    // Expect failure due to existing accepted_offer
    assert!(result.is_err(), "Expected failure but got success");
    assert_eq!(result.unwrap_err(), "Liquidity request already fulfilled");
}

#[test]
fn test_try_accept_liquidity_request_fails_if_lender_is_owner() {
    // Setup context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create token account ID
    let token: AccountId = "usdc.token.near".parse().unwrap();

    // Create a valid liquidity request
    let request = create_valid_liquidity_request(token.clone());

    // Initialize vault with the liquidity request
    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());

    // Construct a matching accept message
    let msg = apply_counter_offer_message_from(&request);

    // Attempt to accept as the vault owner
    let result = contract.try_accept_liquidity_request(owner(), request.amount, msg, token);

    // Expect failure due to self-lending
    assert!(result.is_err(), "Expected failure but got success");
    assert_eq!(
        result.unwrap_err(),
        "Vault owner cannot fulfill their own request"
    );
}

#[test]
fn test_try_accept_liquidity_request_fails_if_token_contract_mismatch() {
    // Setup context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Create token account ID
    let token: AccountId = "usdc.token.near".parse().unwrap();
    let wrong_token_contract: AccountId = "other.token.near".parse().unwrap();

    // Create a valid liquidity request
    let request = create_valid_liquidity_request(token.clone());

    // Initialize vault with the liquidity request
    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());

    // Construct a matching accept message (token matches)
    let msg = apply_counter_offer_message_from(&request);

    // Call with wrong token contract (predecessor)
    let result =
        contract.try_accept_liquidity_request(alice(), request.amount, msg, wrong_token_contract);

    // Expect failure due to token mismatch
    assert!(result.is_err(), "Expected failure but got success");
    assert_eq!(result.unwrap_err(), "Token mismatch");
}

#[test]
fn test_try_accept_liquidity_request_fails_if_msg_token_mismatch() {
    // Setup context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Token in the liquidity request
    let correct_token: AccountId = "usdc.token.near".parse().unwrap();

    // Different token in the lender's message
    let wrong_msg_token: AccountId = "dai.token.near".parse().unwrap();

    // Create a valid liquidity request
    let request = create_valid_liquidity_request(correct_token.clone());

    // Initialize vault with the liquidity request
    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());

    // Construct accept message with wrong token
    let mut msg = apply_counter_offer_message_from(&request);
    msg.token = wrong_msg_token;

    // Call using correct token contract as predecessor, but wrong msg.token
    let result = contract.try_accept_liquidity_request(alice(), request.amount, msg, correct_token);

    // Expect failure due to token mismatch
    assert!(result.is_err(), "Expected failure but got success");
    assert_eq!(result.unwrap_err(), "Token mismatch");
}

#[test]
fn test_try_accept_liquidity_request_fails_if_msg_amount_mismatch() {
    // Setup context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Setup token and liquidity request
    let token: AccountId = "usdc.token.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());

    // Initialize vault with valid request
    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());

    // Create accept message with wrong amount
    let mut msg = apply_counter_offer_message_from(&request);
    msg.amount = U128(2_000_000); // mismatch here

    // Attempt to accept
    let result = contract.try_accept_liquidity_request(
        alice(),
        request.amount, // correct attached amount
        msg,
        token,
    );

    // Expect failure due to amount mismatch
    assert!(result.is_err(), "Expected failure but got success");
    assert_eq!(result.unwrap_err(), "Amount mismatch");
}

#[test]
fn test_try_accept_liquidity_request_fails_if_attached_amount_mismatch() {
    // Setup context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Setup token and liquidity request
    let token: AccountId = "usdc.token.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());

    // Initialize vault with valid request
    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());

    // Construct a valid message (msg.amount is correct)
    let msg = apply_counter_offer_message_from(&request);

    // Pass wrong attached amount (e.g. off by 1)
    let wrong_attached_amount = U128(request.amount.0 - 1);

    let result = contract.try_accept_liquidity_request(
        alice(),
        wrong_attached_amount, // mismatch here
        msg,
        token,
    );

    // Expect failure due to attached amount mismatch
    assert!(result.is_err(), "Expected failure but got success");
    assert_eq!(result.unwrap_err(), "Amount mismatch");
}

#[test]
fn test_try_accept_liquidity_request_fails_if_interest_mismatch() {
    // Setup context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Setup token and liquidity request
    let token: AccountId = "usdc.token.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());

    // Initialize vault with valid request
    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());

    // Construct message with mismatched interest
    let mut msg = apply_counter_offer_message_from(&request);
    msg.interest = U128(request.interest.0 + 1); // mismatch here

    // Call with correct attached amount
    let result = contract.try_accept_liquidity_request(alice(), request.amount, msg, token);

    // Expect failure due to interest mismatch
    assert!(result.is_err(), "Expected failure but got success");
    assert_eq!(result.unwrap_err(), "Interest mismatch");
}

#[test]
fn test_try_accept_liquidity_request_fails_if_collateral_mismatch() {
    // Setup context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Setup token and liquidity request
    let token: AccountId = "usdc.token.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());

    // Initialize vault with valid request
    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());

    // Construct message with mismatched collateral
    let mut msg = apply_counter_offer_message_from(&request);
    msg.collateral = NearToken::from_near(6); // mismatch here

    // Call with correct attached amount
    let result = contract.try_accept_liquidity_request(alice(), request.amount, msg, token);

    // Expect failure due to collateral mismatch
    assert!(result.is_err(), "Expected failure but got success");
    assert_eq!(result.unwrap_err(), "Collateral mismatch");
}

#[test]
fn test_try_accept_liquidity_request_fails_if_duration_mismatch() {
    // Setup context
    let ctx = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(ctx);

    // Setup token and liquidity request
    let token: AccountId = "usdc.token.near".parse().unwrap();
    let request = create_valid_liquidity_request(token.clone());

    // Initialize vault with valid request
    let mut contract = Vault::new(owner(), 0, 1);
    contract.liquidity_request = Some(request.clone());

    // Construct message with mismatched duration
    let mut msg = apply_counter_offer_message_from(&request);
    msg.duration = request.duration + 1; // mismatch here

    // Call with correct attached amount
    let result = contract.try_accept_liquidity_request(alice(), request.amount, msg, token);

    // Expect failure due to duration mismatch
    assert!(result.is_err(), "Expected failure but got success");
    assert_eq!(result.unwrap_err(), "Duration mismatch");
}
