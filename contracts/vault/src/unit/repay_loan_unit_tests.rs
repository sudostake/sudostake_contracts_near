use near_sdk::{
    collections::UnorderedMap, json_types::U128, testing_env, AccountId, NearToken, PromiseError,
};
use test_utils::{alice, get_context, get_context_with_timestamp, owner};

use crate::{
    contract::Vault,
    types::{
        AcceptedOffer, CounterOffer, Liquidation, LiquidityRequest, PendingLiquidityRequest,
        ProcessingState, StorageKey, LOCK_TIMEOUT,
    },
};

#[path = "test_utils.rs"]
mod test_utils;

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_repay_loan_fails_without_yocto() {
    // Simulate environment with no attached deposit
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault with accepted loan and liquidity request
    let mut vault = Vault::new(owner(), 0, 1);
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.testnet".parse().unwrap(),
        amount: 1_000_000.into(),
        interest: 100_000.into(),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 1234567890,
    });
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.testnet".parse().unwrap(),
        accepted_at: 1234567890,
    });

    // Attempt to call repay_loan without 1 yoctoNEAR — should panic
    vault.repay_loan();
}

#[test]
#[should_panic(expected = "Only the vault owner can repay the loan")]
fn test_repay_loan_fails_if_not_owner() {
    // Set context where `alice` is the caller (not the owner)
    let context = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault owned by `owner`, with loan state
    let mut vault = Vault::new(owner(), 0, 1);
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.testnet".parse().unwrap(),
        amount: 1_000_000.into(),
        interest: 100_000.into(),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 1234567890,
    });
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.testnet".parse().unwrap(),
        accepted_at: 1234567890,
    });

    // Alice tries to repay — should panic
    vault.repay_loan();
}

#[test]
#[should_panic(expected = "No active loan to repay")]
fn test_repay_loan_fails_if_no_liquidity_request() {
    // Set context as vault owner with 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Vault has no liquidity_request (and no accepted_offer — consistent state)
    let mut vault = Vault::new(owner(), 0, 1);

    // Should panic: no loan to repay
    vault.repay_loan();
}

#[test]
#[should_panic(expected = "No accepted offer found")]
fn test_repay_loan_fails_if_accepted_offer_is_none() {
    // Set context with vault owner and 1 yoctoNEAR attached
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Vault has liquidity_request but no accepted_offer
    let mut vault = Vault::new(owner(), 0, 1);
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.testnet".parse().unwrap(),
        amount: 1_000_000.into(),
        interest: 100_000.into(),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 1234567890,
    });

    // Should panic due to missing accepted_offer
    vault.repay_loan();
}

#[test]
#[should_panic(expected = "Loan has already entered liquidation")]
fn test_repay_loan_fails_if_liquidation_is_active() {
    // Set up context with vault owner and 1 yoctoNEAR deposit
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize a valid vault loan state
    let mut vault = Vault::new(owner(), 0, 1);
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.testnet".parse().unwrap(),
        amount: 1_000_000.into(),
        interest: 100_000.into(),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 1234567890,
    });
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.testnet".parse().unwrap(),
        accepted_at: 1234567890,
    });

    // Simulate that liquidation has started
    vault.liquidation = Some(Liquidation {
        liquidated: NearToken::from_yoctonear(500_000),
    });

    // Attempt repay_loan — should panic
    vault.repay_loan();
}

#[test]
fn test_repay_loan_sets_processing_lock() {
    // Set up context with owner and 1 yoctoNEAR attached
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Prepare vault with active loan and offer
    let mut vault = Vault::new(owner(), 0, 1);
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.testnet".parse().unwrap(),
        amount: 1_000_000.into(),
        interest: 100_000.into(),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 1234567890,
    });
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.testnet".parse().unwrap(),
        accepted_at: 1234567890,
    });

    // Call repay_loan — should acquire lock
    vault.repay_loan();

    // Assert lock was acquired with correct state
    assert_eq!(
        vault.processing_state,
        ProcessingState::RepayLoan,
        "Expected vault to enter RepayLoan processing state"
    );
}

#[test]
fn test_on_repay_loan_success_clears_state() {
    // Simulated block timestamp
    let now = 1_000_000_000_000_000;

    // Set test context with controlled timestamp
    let context = get_context_with_timestamp(owner(), NearToken::from_near(10), None, Some(now));
    testing_env!(context);

    // Set up a vault in RepayLoan state
    let mut vault = Vault::new(owner(), 0, 1);
    vault.processing_state = ProcessingState::RepayLoan;
    vault.processing_since = now;
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.testnet".parse().unwrap(),
        amount: 1_000_000.into(),
        interest: 100_000.into(),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: now,
    });
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.testnet".parse().unwrap(),
        accepted_at: now,
    });

    // Simulate successful repayment callback
    vault.on_repay_loan(Ok(()));

    // Assert loan state is cleared
    assert!(
        vault.liquidity_request.is_none(),
        "Liquidity request should be cleared"
    );
    assert!(
        vault.accepted_offer.is_none(),
        "Accepted offer should be cleared"
    );

    // Assert lock is released
    assert_eq!(vault.processing_state, ProcessingState::Idle);
    assert_eq!(vault.processing_since, 0);
}

#[test]
fn test_on_repay_loan_success_clears_counter_offers_storage() {
    let now = 1_000_000_000_000_000;
    let context = get_context_with_timestamp(owner(), NearToken::from_near(10), None, Some(now));
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    vault.processing_state = ProcessingState::RepayLoan;
    vault.processing_since = now;

    let token: AccountId = "usdc.testnet".parse().unwrap();
    vault.liquidity_request = Some(LiquidityRequest {
        token: token.clone(),
        amount: 1_000_000.into(),
        interest: 100_000.into(),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: now,
    });
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.testnet".parse().unwrap(),
        accepted_at: now,
    });

    vault.pending_liquidity_request = Some(PendingLiquidityRequest {
        token: token.clone(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    });

    let mut offers = UnorderedMap::new(StorageKey::CounterOffers);
    offers.insert(
        &alice(),
        &CounterOffer {
            proposer: alice(),
            amount: U128(900_000),
            timestamp: now,
        },
    );
    vault.counter_offers = Some(offers);

    vault.on_repay_loan(Ok(()));

    assert!(vault.counter_offers.is_none(), "Counter offers should be None");
    assert!(
        vault.pending_liquidity_request.is_none(),
        "Pending liquidity request should be cleared"
    );

    let inspector: UnorderedMap<AccountId, CounterOffer> =
        UnorderedMap::new(StorageKey::CounterOffers);
    assert_eq!(
        inspector.len(),
        0,
        "Counter offer storage prefix should be empty after repayment"
    );
}

#[test]
fn test_on_repay_loan_failure_preserves_loan_state() {
    // Simulated block timestamp
    let now = 1_000_000_000_000_000;

    // Set test context with controlled timestamp
    let context = get_context_with_timestamp(owner(), NearToken::from_near(10), None, Some(now));
    testing_env!(context);

    // Set up vault in RepayLoan state
    let mut vault = Vault::new(owner(), 0, 1);
    vault.processing_state = ProcessingState::RepayLoan;
    vault.processing_since = now;

    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.testnet".parse().unwrap(),
        amount: 1_000_000.into(),
        interest: 100_000.into(),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: now,
    });

    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.testnet".parse().unwrap(),
        accepted_at: now,
    });

    // Simulate failure callback
    vault.on_repay_loan(Err(PromiseError::Failed));

    // Loan state should remain
    assert!(
        vault.liquidity_request.is_some(),
        "Liquidity request should remain"
    );
    assert!(
        vault.accepted_offer.is_some(),
        "Accepted offer should remain"
    );

    // Lock should be released
    assert_eq!(vault.processing_state, ProcessingState::Idle);
    assert_eq!(vault.processing_since, 0);
}

#[test]
fn test_repay_loan_clears_stale_lock_and_proceeds() {
    // Timestamp representing now
    let now = 1_000_000_000_000_000;

    // Simulate stale lock acquired before (LOCK_TIMEOUT + 1) ns
    let stale_timestamp = now - LOCK_TIMEOUT - 1;

    // Set up context at current time
    let context = get_context_with_timestamp(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
        Some(now),
    );
    testing_env!(context);

    // Vault has active loan + stale lock
    let mut vault = Vault::new(owner(), 0, 1);
    vault.processing_state = ProcessingState::ProcessClaims;
    vault.processing_since = stale_timestamp;

    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.testnet".parse().unwrap(),
        amount: 1_000_000.into(),
        interest: 100_000.into(),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: now,
    });

    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.testnet".parse().unwrap(),
        accepted_at: now,
    });

    // This should proceed by clearing stale lock
    vault.repay_loan();

    // Assert lock was updated to RepayLoan
    assert_eq!(
        vault.processing_state,
        ProcessingState::RepayLoan,
        "Expected RepayLoan lock to be acquired"
    );
    assert_eq!(
        vault.processing_since, now,
        "Expected lock timestamp to update"
    );
}
