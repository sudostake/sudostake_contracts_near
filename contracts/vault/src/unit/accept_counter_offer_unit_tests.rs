use near_sdk::{collections::UnorderedMap, json_types::U128, testing_env, AccountId, NearToken};
use test_utils::{
    alice, bob, create_valid_liquidity_request, get_context, get_context_with_timestamp, owner,
};

use crate::{
    contract::Vault,
    types::{AcceptedOffer, CounterOfferMessage, LiquidityRequest, StorageKey},
};

#[path = "test_utils.rs"]
mod test_utils;

fn set_env(predecessor: AccountId, deposit: Option<NearToken>) {
    let ctx = get_context(predecessor, NearToken::from_near(10), deposit);
    testing_env!(ctx);
}

fn set_env_with_timestamp(predecessor: AccountId, deposit: Option<NearToken>, timestamp: u64) {
    let ctx = get_context_with_timestamp(
        predecessor,
        NearToken::from_near(10),
        deposit,
        Some(timestamp),
    );
    testing_env!(ctx);
}

fn new_vault_with_request(token: AccountId) -> (Vault, LiquidityRequest) {
    let mut vault = Vault::new(owner(), 0, 1);
    let request = create_valid_liquidity_request(token);
    vault.liquidity_request = Some(request.clone());
    (vault, request)
}

fn counter_offer_message_from(request: &LiquidityRequest) -> CounterOfferMessage {
    CounterOfferMessage {
        action: "NewCounterOffer".to_string(),
        token: request.token.clone(),
        amount: request.amount,
        interest: request.interest,
        collateral: request.collateral,
        duration: request.duration,
    }
}

fn add_counter_offer(
    vault: &mut Vault,
    proposer: AccountId,
    amount: u128,
    request: &LiquidityRequest,
) {
    let msg = counter_offer_message_from(request);
    vault
        .try_add_counter_offer(proposer, U128(amount), msg, request.token.clone())
        .expect("counter offer should be recorded");
}

#[test]
fn accept_counter_offer_updates_request_and_clears_storage() {
    let token: AccountId = "usdc.test.near".parse().unwrap();
    let timestamp = 42_424_u64;
    set_env_with_timestamp(owner(), Some(NearToken::from_yoctonear(1)), timestamp);

    let (mut vault, request) = new_vault_with_request(token.clone());
    add_counter_offer(&mut vault, bob(), 850_000, &request);
    add_counter_offer(&mut vault, alice(), 900_000, &request);

    vault.accept_counter_offer(alice(), U128(900_000));

    let accepted = vault
        .accepted_offer
        .as_ref()
        .expect("expected accepted offer to be recorded");
    assert_eq!(accepted.lender, alice(), "should store the lender account");
    assert_eq!(
        accepted.accepted_at, timestamp,
        "accepted timestamp should use current block timestamp"
    );

    let updated_request = vault
        .liquidity_request
        .as_ref()
        .expect("liquidity request should remain for active loan");
    assert_eq!(
        updated_request.amount,
        U128(900_000),
        "principal should match the accepted counter offer"
    );
    assert_eq!(
        updated_request.token, request.token,
        "token contract must stay unchanged"
    );
    assert_eq!(
        updated_request.interest, request.interest,
        "interest terms should remain intact"
    );
    assert_eq!(
        updated_request.collateral, request.collateral,
        "collateral should not be altered"
    );
    assert_eq!(
        updated_request.duration, request.duration,
        "duration should remain intact"
    );

    assert!(
        vault.counter_offers.is_none(),
        "counter offers should be cleared from state"
    );

    let inspector: UnorderedMap<AccountId, crate::types::CounterOffer> =
        UnorderedMap::new(StorageKey::CounterOffers);
    assert_eq!(
        inspector.len(),
        0,
        "underlying storage should be empty after acceptance"
    );
}

#[test]
fn accept_counter_offer_with_single_entry_clears_option() {
    let token: AccountId = "usdc.test.near".parse().unwrap();
    set_env(owner(), Some(NearToken::from_yoctonear(1)));

    let (mut vault, request) = new_vault_with_request(token.clone());
    add_counter_offer(&mut vault, alice(), 900_000, &request);

    vault.accept_counter_offer(alice(), U128(900_000));

    assert!(
        vault.counter_offers.is_none(),
        "counter offers option should be None after acceptance"
    );

    let inspector: UnorderedMap<AccountId, crate::types::CounterOffer> =
        UnorderedMap::new(StorageKey::CounterOffers);
    assert_eq!(
        inspector.len(),
        0,
        "underlying storage must be empty even when only a single offer existed"
    );
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn accept_counter_offer_requires_exact_deposit() {
    set_env(owner(), None);
    let token: AccountId = "usdc.test.near".parse().unwrap();
    let (mut vault, request) = new_vault_with_request(token);
    add_counter_offer(&mut vault, alice(), 900_000, &request);

    vault.accept_counter_offer(alice(), U128(900_000));
}

#[test]
#[should_panic(expected = "Only the vault owner can accept a counter offer")]
fn accept_counter_offer_rejects_non_owner() {
    set_env(alice(), Some(NearToken::from_yoctonear(1)));

    let token: AccountId = "usdc.test.near".parse().unwrap();
    let (mut vault, request) = new_vault_with_request(token);
    add_counter_offer(&mut vault, alice(), 900_000, &request);

    vault.accept_counter_offer(alice(), U128(900_000));
}

#[test]
#[should_panic(expected = "No liquidity request available")]
fn accept_counter_offer_requires_liquidity_request() {
    set_env(owner(), Some(NearToken::from_yoctonear(1)));
    let mut vault = Vault::new(owner(), 0, 1);

    vault.accept_counter_offer(alice(), U128(900_000));
}

#[test]
#[should_panic(expected = "Liquidity request already accepted")]
fn accept_counter_offer_rejects_when_already_matched() {
    set_env(owner(), Some(NearToken::from_yoctonear(1)));

    let token: AccountId = "usdc.test.near".parse().unwrap();
    let (mut vault, request) = new_vault_with_request(token);
    vault.accepted_offer = Some(AcceptedOffer {
        lender: bob(),
        accepted_at: 99,
    });
    add_counter_offer(&mut vault, alice(), 900_000, &request);

    vault.accept_counter_offer(alice(), U128(900_000));
}

#[test]
#[should_panic(expected = "No counter offers available")]
fn accept_counter_offer_requires_active_offers() {
    set_env(owner(), Some(NearToken::from_yoctonear(1)));

    let token: AccountId = "usdc.test.near".parse().unwrap();
    let (mut vault, _) = new_vault_with_request(token);

    vault.accept_counter_offer(alice(), U128(900_000));
}

#[test]
#[should_panic(expected = "Counter offer from proposer not found")]
fn accept_counter_offer_requires_existing_proposer_entry() {
    set_env(owner(), Some(NearToken::from_yoctonear(1)));

    let token: AccountId = "usdc.test.near".parse().unwrap();
    let (mut vault, request) = new_vault_with_request(token);
    add_counter_offer(&mut vault, bob(), 850_000, &request);

    vault.accept_counter_offer(alice(), U128(900_000));
}

#[test]
#[should_panic(expected = "Provided amount does not match the counter offer")]
fn accept_counter_offer_rejects_amount_mismatch() {
    set_env(owner(), Some(NearToken::from_yoctonear(1)));

    let token: AccountId = "usdc.test.near".parse().unwrap();
    let (mut vault, request) = new_vault_with_request(token);
    add_counter_offer(&mut vault, alice(), 900_000, &request);

    vault.accept_counter_offer(alice(), U128(800_000));
}
