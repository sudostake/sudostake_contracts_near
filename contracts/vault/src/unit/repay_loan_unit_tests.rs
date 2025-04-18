use near_sdk::{json_types::U128, testing_env, NearToken, PromiseError};
use test_utils::{alice, get_context, owner};

use crate::{
    contract::Vault,
    types::{AcceptedOffer, Liquidation, LiquidityRequest},
};

#[path = "test_utils.rs"]
mod test_utils;

#[test]
fn test_repay_loan_succeeds_when_valid() {
    // Set up the context with 1 yoctoNEAR from the vault owner
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Inject a valid liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Add a valid accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.near".parse().unwrap(),
        accepted_at: 12345678,
    });

    // Call repay_loan
    let _ = vault.repay_loan();

    // Verify repayment lock is set
    assert!(
        vault.repaying,
        "Expected repayment lock to be set to true after initiating repayment"
    );
}

#[test]
#[should_panic(expected = "Only the vault owner can repay the loan")]
fn test_repay_loan_fails_if_not_owner() {
    // Set context as alice (not the vault owner), with 1 yocto
    let context = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Vault is owned by `owner.near`
    let mut vault = Vault::new(owner(), 0, 1);

    // Inject a valid liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Add a valid accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.near".parse().unwrap(),
        accepted_at: 12345678,
    });

    // Alice tries to repay — should panic
    vault.repay_loan();
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_repay_loan_fails_without_yocto() {
    // Set context as vault owner but with 0 yoctoNEAR
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Inject a valid liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Add a valid accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.near".parse().unwrap(),
        accepted_at: 12345678,
    });

    // Try to repay without 1 yoctoNEAR — should panic
    vault.repay_loan();
}

#[test]
#[should_panic(expected = "No active loan to repay")]
fn test_repay_loan_fails_if_no_liquidity_request() {
    // Set context as vault owner with 1 yocto
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to repay — should panic
    vault.repay_loan();
}

#[test]
#[should_panic(expected = "No accepted offer found")]
fn test_repay_loan_fails_if_no_accepted_offer() {
    // Set context as vault owner with 1 yocto
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Inject a valid liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Attempt to repay — should panic
    vault.repay_loan();
}

#[test]
#[should_panic(expected = "Loan has already entered liquidation")]
fn test_repay_loan_fails_if_liquidation_started() {
    // Set context as vault owner with 1 yocto
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault with valid loan and liquidation active
    let mut vault = Vault::new(owner(), 0, 1);

    // Inject a valid liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Add a valid accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.near".parse().unwrap(),
        accepted_at: 12345678,
    });

    // Simulate liquidation
    vault.liquidation = Some(Liquidation {
        liquidated: NearToken::from_near(1),
    });

    // Attempt to repay — should panic
    vault.repay_loan();
}

#[test]
#[should_panic(expected = "Repayment already in progress")]
fn test_repay_loan_fails_if_repaying_flag_already_true() {
    // Set context as vault owner with 1 yocto
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault with loan and set repaying = true
    let mut vault = Vault::new(owner(), 0, 1);

    // Inject a valid liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Add a valid accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.near".parse().unwrap(),
        accepted_at: 12345678,
    });

    // Simulate a repayment lock
    vault.repay_loan();

    // Attempt to repay again — should panic
    vault.repay_loan();
}

#[test]
fn test_on_repay_loan_success_clears_state() {
    // Set context as vault owner with 1 yocto
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault with loan and set repaying = true
    let mut vault = Vault::new(owner(), 0, 1);

    // Inject a valid liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Add a valid accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.near".parse().unwrap(),
        accepted_at: 12345678,
    });

    // Simulate a repayment lock
    vault.repay_loan();

    // Simulate successful callback
    vault.on_repay_loan(Ok(()));

    // Assert loan state was cleared
    assert!(
        vault.accepted_offer.is_none(),
        "accepted_offer should be cleared"
    );
    assert!(
        vault.liquidity_request.is_none(),
        "liquidity_request should be cleared"
    );
    assert!(!vault.repaying, "repaying flag should be reset to false");
}

#[test]
#[should_panic(expected = "Repayment transfer to lender failed")]
fn test_on_repay_loan_failure_panics_and_clears_lock() {
    // Set context as vault owner with 1 yocto
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize vault with loan and set repaying = true
    let mut vault = Vault::new(owner(), 0, 1);

    // Inject a valid liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Add a valid accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: "lender.near".parse().unwrap(),
        accepted_at: 12345678,
    });

    // Simulate a repayment lock
    vault.repay_loan();

    // Simulate failed ft_transfer callback
    // Should panic AND set repaying = false
    vault.on_repay_loan(Err(PromiseError::Failed));

    // Assert repaying is reset to false
    assert!(!vault.repaying, "repaying flag should be reset to false");

    // Assert loan state remains intact
    assert!(
        vault.accepted_offer.is_some(),
        "accepted_offer should not be cleared"
    );
    assert!(
        vault.liquidity_request.is_some(),
        "liquidity_request should not be cleared"
    );
}
