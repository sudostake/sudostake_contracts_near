use near_sdk::{
    collections::UnorderedMap, json_types::U128, test_utils::VMContextBuilder, test_vm_config,
    testing_env, AccountId, NearToken, PromiseResult, RuntimeFeesConfig,
};
use test_utils::{
    alice, bob, create_valid_liquidity_request, get_context, get_context_with_timestamp, owner,
};

use crate::{
    contract::Vault,
    types::{AcceptedOffer, CounterOfferMessage, LiquidityRequest, RefundBatchItem, StorageKey},
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

fn set_env_with_promise_results(
    predecessor: AccountId,
    deposit: Option<NearToken>,
    results: Vec<PromiseResult>,
) {
    let mut builder = VMContextBuilder::new();
    builder
        .predecessor_account_id(predecessor.clone())
        .signer_account_id(predecessor)
        .current_account_id(owner())
        .account_balance(NearToken::from_near(10));

    if let Some(dep) = deposit {
        builder.attached_deposit(dep);
    }

    let context = builder.build();
    testing_env!(
        context,
        test_vm_config(),
        RuntimeFeesConfig::test(),
        Default::default(),
        results
    );
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
fn accept_counter_offer_with_no_other_offers_does_not_queue_refunds() {
    let token: AccountId = "usdc.test.near".parse().unwrap();
    set_env(owner(), Some(NearToken::from_yoctonear(1)));

    let (mut vault, request) = new_vault_with_request(token.clone());
    add_counter_offer(&mut vault, alice(), 900_000, &request);

    vault.accept_counter_offer(alice(), U128(900_000));

    assert_eq!(
        vault.refund_list.iter().count(),
        0,
        "No refund entries should be recorded when there are no competing offers"
    );
}

#[test]
fn accept_counter_offer_records_refunds_for_remaining_offers() {
    let token: AccountId = "usdc.test.near".parse().unwrap();
    set_env(owner(), Some(NearToken::from_yoctonear(1)));

    let (mut vault, request) = new_vault_with_request(token.clone());
    add_counter_offer(&mut vault, bob(), 800_000, &request);
    let carol: AccountId = "carol.near".parse().unwrap();
    add_counter_offer(&mut vault, carol.clone(), 850_000, &request);
    add_counter_offer(&mut vault, alice(), 900_000, &request);

    vault.accept_counter_offer(alice(), U128(900_000));

    let mut refunds: Vec<(AccountId, u128, Option<AccountId>)> = vault
        .refund_list
        .iter()
        .map(|(_, entry)| (entry.proposer.clone(), entry.amount.0, entry.token.clone()))
        .collect();
    refunds.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(
        refunds,
        vec![
            (bob(), 800_000, Some(token.clone())),
            (carol.clone(), 850_000, Some(token.clone()))
        ],
        "refund_list should retain entries for every non-accepted offer"
    );
}

#[test]
fn accept_counter_offer_single_refund_clears_after_callback() {
    let token: AccountId = "usdc.test.near".parse().unwrap();
    set_env(owner(), Some(NearToken::from_yoctonear(1)));

    let (mut vault, request) = new_vault_with_request(token.clone());
    add_counter_offer(&mut vault, bob(), 800_000, &request);
    add_counter_offer(&mut vault, alice(), 900_000, &request);

    vault.accept_counter_offer(alice(), U128(900_000));

    let entries: Vec<(u64, crate::types::RefundEntry)> = vault.refund_list.iter().collect();
    assert_eq!(
        entries.len(),
        1,
        "Accepting should schedule exactly one refund for the remaining offer"
    );

    let (refund_id, entry) = &entries[0];

    set_env(owner(), None);
    vault.on_refund_complete(
        *refund_id,
        entry.proposer.clone(),
        entry.amount,
        entry
            .token
            .clone()
            .expect("Refund entry should retain the token address"),
        Ok(()),
    );

    assert!(
        vault.refund_list.get(refund_id).is_none(),
        "Successful callback should clear the single refund entry"
    );
}

#[test]
fn accept_counter_offer_batch_refunds_requeues_failures() {
    let token: AccountId = "usdc.test.near".parse().unwrap();
    set_env(owner(), Some(NearToken::from_yoctonear(1)));

    let (mut vault, request) = new_vault_with_request(token.clone());
    add_counter_offer(&mut vault, bob(), 800_000, &request);
    let carol: AccountId = "carol.near".parse().unwrap();
    add_counter_offer(&mut vault, carol.clone(), 850_000, &request);
    add_counter_offer(&mut vault, alice(), 900_000, &request);

    vault.accept_counter_offer(alice(), U128(900_000));

    let mut metadata: Vec<RefundBatchItem> = vault
        .refund_list
        .iter()
        .map(|(id, entry)| (id, entry.proposer.clone(), entry.amount))
        .collect();
    metadata.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        metadata.len(),
        2,
        "Two competing offers should produce two refund metadata entries"
    );
    let failed_entry = metadata[1].clone();

    set_env_with_promise_results(
        owner(),
        None,
        vec![PromiseResult::Successful(vec![]), PromiseResult::Failed],
    );

    vault.on_batch_refunds_complete(token.clone(), metadata);

    let mut remaining: Vec<(AccountId, u128, Option<AccountId>)> = vault
        .refund_list
        .iter()
        .map(|(_, entry)| (entry.proposer.clone(), entry.amount.0, entry.token.clone()))
        .collect();
    remaining.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(
        remaining,
        vec![(
            failed_entry.1.clone(),
            failed_entry.2 .0,
            Some(token.clone())
        )],
        "Only the failed refund should remain queued after the batch callback runs"
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
