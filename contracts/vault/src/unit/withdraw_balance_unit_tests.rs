#[path = "test_utils.rs"]
mod test_utils;

use crate::contract::Vault;
use crate::types::{AcceptedOffer, Liquidation, LiquidityRequest, RefundEntry, STORAGE_BUFFER};
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::{env, AccountId, NearToken};
use near_sdk::{json_types::U128, testing_env};
use test_utils::{alice, get_context, insert_refund_entry, owner};

#[test]
fn owner_can_withdraw_near_successfully() {
    // Set up context with vault owner and 5 NEAR in balance
    let context = get_context(
        owner(),
        NearToken::from_near(5),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to withdraw 1 NEAR
    vault.withdraw_balance(
        None, // native NEAR
        U128::from(NearToken::from_near(1).as_yoctonear()),
        None, // default to owner
    );
}

#[test]
#[should_panic(expected = "Not enough NEAR balance")]
fn withdraw_near_insufficient_balance_should_panic() {
    // Set up context with 1 NEAR in the vault
    let context = get_context(
        owner(),
        NearToken::from_near(1),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to withdraw more than available (e.g., 5 NEAR)
    vault.withdraw_balance(
        None,
        U128::from(NearToken::from_near(5).as_yoctonear()),
        None,
    );
}

#[test]
#[should_panic(expected = "Only the vault owner can withdraw")]
fn non_owner_cannot_withdraw_should_panic() {
    // Set up context with `alice` as the caller
    // Vault account has 10 NEAR in balance
    let context = get_context(
        alice(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault with a different owner (`owner`)
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to withdraw 1 NEAR from the vault
    // Since the caller is not the owner, this should panic
    vault.withdraw_balance(
        None,
        U128::from(NearToken::from_near(1).as_yoctonear()),
        None,
    );
}

#[test]
#[should_panic(expected = "Cannot withdraw while there are pending refund entries")]
fn test_disallow_withdrawal_if_refund_list_not_empty() {
    // Setup context
    let context = get_context(
        owner(),
        NearToken::from_near(5),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Create vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Simulate refund_list has one pending entry
    insert_refund_entry(
        &mut vault,
        0,
        RefundEntry {
            token: Some("usdc.near".parse().unwrap()),
            proposer: alice(),
            amount: U128(1_000_000),
            added_at_epoch: 0,
        },
    );

    // Attempt to withdraw 1 NEAR from the vault
    // Since there is a pending RefundEntry, this should panic
    vault.withdraw_balance(
        None,
        U128::from(NearToken::from_near(1).as_yoctonear()),
        None,
    );
}

#[test]
#[should_panic(expected = "Cannot withdraw while there are pending refund entries")]
fn test_disallow_nep_withdrawal_if_refund_list_not_empty() {
    let context = get_context(
        owner(),
        NearToken::from_near(5),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);

    insert_refund_entry(
        &mut vault,
        0,
        RefundEntry {
            token: Some("usdc.near".parse().unwrap()),
            proposer: alice(),
            amount: U128(1_000_000),
            added_at_epoch: 0,
        },
    );

    vault.withdraw_balance(
        Some("usdc.near".parse().unwrap()),
        U128::from(1_000_000_u128),
        None,
    );
}

#[test]
fn owner_can_withdraw_nep141_with_one_yocto() {
    // Set up context with vault owner and attach 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize the vault with owner
    let mut vault = Vault::new(owner(), 0, 1);

    // Use a fake token address for NEP-141 (we're not testing the external contract)
    let fake_token: AccountId = "usdc.mock.near".parse().unwrap();

    // Attempt to withdraw 100 USDC tokens (or whatever you want)
    vault.withdraw_balance(
        Some(fake_token),
        U128::from(100_000_000),
        None, // recipient defaults to owner
    );
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn nep141_withdraw_without_one_yocto_should_panic() {
    // Set up context with vault owner, but no attached deposit
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(0)),
    );
    testing_env!(context);

    // Initialize the vault
    let mut vault = Vault::new(owner(), 0, 1);

    // Use a dummy token address to simulate a NEP-141 withdrawal
    let token: AccountId = "usdc.mock.near".parse().unwrap();

    // Attempt to withdraw tokens without attaching 1 yoctoNEAR
    // This should trigger assert_one_yocto() and panic
    vault.withdraw_balance(
        Some(token),
        U128::from(100_000_000),
        None, // recipient defaults to owner
    );
}

#[test]
fn test_get_available_balance_subtracts_storage_and_buffer() {
    // Setup context
    let context = VMContextBuilder::new()
        .current_account_id(owner())
        .account_balance(NearToken::from_near(1))
        .storage_usage(10_000) // e.g., 10 KB
        .build();
    testing_env!(context);

    // create a vault instance
    let vault = Vault::new(owner(), 0, 1);

    // Calculate expected balance
    let storage_cost = env::storage_byte_cost().as_yoctonear() * 10_000;
    let expected =
        1_000_000_000_000_000_000_000_000u128.saturating_sub(storage_cost + STORAGE_BUFFER);

    // Assert balance as expected
    assert_eq!(
        vault.get_available_balance().as_yoctonear(),
        expected,
        "Should subtract both storage cost and buffer"
    );
}

#[test]
fn test_allow_withdraw_when_no_liquidity_request() {
    // Setup context
    let context = get_context(owner(), NearToken::from_near(5), None);
    testing_env!(context);

    // create a vault instance
    let vault = Vault::new(owner(), 0, 1);

    // Test ensure_owner_can_withdraw all token types
    vault.ensure_owner_can_withdraw(None);
    vault.ensure_owner_can_withdraw(Some(&"usdc.near".parse().unwrap()));
}

#[test]
#[should_panic(expected = "Cannot withdraw requested token while counter offers are pending")]
fn test_disallow_withdraw_requested_token_when_counter_offers_pending() {
    // Setup context
    let context = get_context(owner(), NearToken::from_near(5), None);
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    });

    // Panic when we try to withdraw request.token type
    vault.ensure_owner_can_withdraw(Some(&"usdc.near".parse().unwrap()));
}

#[test]
#[should_panic(expected = "Cannot withdraw NEAR while liquidation is in progress")]
fn test_disallow_withdraw_near_during_liquidation() {
    // Setup context
    let context = get_context(owner(), NearToken::from_near(5), None);
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.near".parse().unwrap(),
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
        liquidated: NearToken::from_yoctonear(0),
    });

    // Panic when we try to withdraw collateral
    vault.ensure_owner_can_withdraw(None);
}

#[test]
fn test_allow_near_after_offer_accepted_before_liquidation() {
    // Setup context
    let context = get_context(owner(), NearToken::from_near(5), None);
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.near".parse().unwrap(),
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

    // Owner can still withdraw NEAR tokens even when
    // a liquidity request is active but liquidation has not kicked off
    vault.ensure_owner_can_withdraw(None);
}

#[test]
fn test_allow_nep141_during_liquidation() {
    // Setup context
    let context = get_context(owner(), NearToken::from_near(5), None);
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Add a liquidity request
    vault.liquidity_request = Some(LiquidityRequest {
        token: "usdc.near".parse().unwrap(),
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
        liquidated: NearToken::from_yoctonear(0),
    });

    // NEP-141 withdrawal allowed even during liquidation
    vault.ensure_owner_can_withdraw(Some(&"usdt.near".parse().unwrap()));
}
