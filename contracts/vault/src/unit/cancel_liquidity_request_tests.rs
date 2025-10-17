use near_sdk::{
    collections::UnorderedMap, json_types::U128, test_utils::get_logs, testing_env, AccountId,
    NearToken,
};
use test_utils::{create_valid_liquidity_request, get_context, owner};

use crate::{
    contract::Vault,
    types::{AcceptedOffer, CounterOffer, RefundEntry, StorageKey},
    unit::test_utils::{alice, bob},
};

#[path = "test_utils.rs"]
mod test_utils;

const TOKEN_ID: &str = "usdc.token.near";

fn with_context(predecessor: AccountId, deposit_yocto: u128) {
    let ctx = get_context(
        predecessor,
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(deposit_yocto)),
    );
    testing_env!(ctx);
}

fn with_owner_context(deposit_yocto: u128) {
    with_context(owner(), deposit_yocto);
}

fn vault_with_request() -> Vault {
    let mut vault = Vault::new(owner(), 0, 1);
    vault.liquidity_request = Some(create_valid_liquidity_request(
        TOKEN_ID.parse().expect("valid token id"),
    ));
    vault
}

fn vault_with_counter_offers(
    offers: impl IntoIterator<Item = (AccountId, u128)>,
) -> (Vault, Vec<RefundEntry>) {
    let mut vault = vault_with_request();
    let mut counter_map = UnorderedMap::new(StorageKey::CounterOffers);
    let mut expected_refunds = Vec::new();

    for (idx, (account, amount)) in offers.into_iter().enumerate() {
        let offer_amount = U128(amount);
        let offer = CounterOffer {
            proposer: account.clone(),
            amount: offer_amount,
            timestamp: idx as u64,
        };
        counter_map.insert(&account, &offer);
        expected_refunds.push(RefundEntry {
            token: Some(TOKEN_ID.parse().expect("valid token id")),
            proposer: account,
            amount: offer_amount,
            added_at_epoch: 0,
        });
    }

    if counter_map.is_empty() {
        (vault, expected_refunds)
    } else {
        vault.counter_offers = Some(counter_map);
        (vault, expected_refunds)
    }
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_cancel_liquidity_request_fails_if_missing_yocto() {
    // Setup context with 0 attached deposit
    with_owner_context(0);

    // Create vault with a liquidity request
    let mut vault = vault_with_request();

    // Should panic due to missing 1 yoctoNEAR
    vault.cancel_liquidity_request();
}

#[test]
#[should_panic(expected = "Only the vault owner can cancel the liquidity request")]
fn test_cancel_liquidity_request_fails_if_not_owner() {
    // Setup context where alice (not the owner) tries to cancel
    with_context(alice(), 1);

    // Create vault owned by "owner"
    let mut vault = vault_with_request();

    // Should panic due to unauthorized caller
    vault.cancel_liquidity_request();
}

#[test]
#[should_panic(expected = "No active liquidity request")]
fn test_cancel_liquidity_request_fails_if_no_request() {
    // Setup context as owner with correct deposit
    with_owner_context(1);

    // Create vault with no liquidity request
    let mut vault = Vault::new(owner(), 0, 1);

    // Should panic due to no active liquidity request
    vault.cancel_liquidity_request();
}

#[test]
#[should_panic(expected = "Cannot cancel after an offer has been accepted")]
fn test_cancel_liquidity_request_fails_if_offer_accepted() {
    // Setup context as vault owner with 1 yoctoNEAR attached
    with_owner_context(1);

    // Create a vault owned by the caller
    let mut vault = vault_with_request();

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
    with_owner_context(1);

    // Create a vault owned by the caller
    let (mut vault, _) = vault_with_counter_offers(vec![(alice(), 1_000_000)]);

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
    with_owner_context(1);

    let (mut vault, _) = vault_with_counter_offers(vec![(alice(), 1_000_000)]);

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
    with_owner_context(1);

    // Create a vault owned by the caller
    let (mut vault, _) = vault_with_counter_offers(Vec::<(AccountId, u128)>::new());
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

#[test]
fn test_cancel_liquidity_request_records_refunds_for_each_offer() {
    with_owner_context(1);

    let (mut vault, expected_refunds) =
        vault_with_counter_offers(vec![(alice(), 1_000_000), (bob(), 2_000_000)]);

    vault.cancel_liquidity_request();

    let mut refunds: Vec<_> = vault
        .refund_list
        .iter()
        .map(|(_, entry)| (entry.proposer.clone(), entry.amount))
        .collect();
    refunds.sort_by(|a, b| a.0.cmp(&b.0));

    let mut expected: Vec<_> = expected_refunds
        .into_iter()
        .map(|entry| (entry.proposer, entry.amount))
        .collect();
    expected.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(
        refunds, expected,
        "Each counter offer should be materialised as a refund entry"
    );
}

#[test]
fn test_cancel_liquidity_request_leaves_refund_list_empty_without_offers() {
    with_owner_context(1);

    let (mut vault, _) = vault_with_counter_offers(Vec::<(AccountId, u128)>::new());

    vault.cancel_liquidity_request();

    assert!(
        vault.refund_list.is_empty(),
        "Refund list must stay empty when no counter offers exist"
    );
}

#[test]
fn test_cancel_liquidity_request_emits_structured_event() {
    with_owner_context(1);

    let (mut vault, _) = vault_with_counter_offers(vec![(alice(), 1_000_000)]);

    vault.cancel_liquidity_request();

    let logs = get_logs().join("");
    assert!(
        logs.contains(r#""event":"liquidity_request_cancelled""#),
        "Expected liquidity_request_cancelled event. Logs: {logs}"
    );
}
