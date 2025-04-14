#![allow(dead_code)]
use near_sdk::{json_types::U128, test_utils::VMContextBuilder, AccountId, NearToken};

use crate::types::LiquidityRequest;

pub fn alice() -> AccountId {
    "alice.near".parse().unwrap()
}

pub fn owner() -> AccountId {
    "owner.near".parse().unwrap()
}

pub fn get_context(
    predecessor: AccountId,
    account_balance: NearToken,
    attached_deposit: Option<NearToken>,
) -> near_sdk::VMContext {
    // Step 1: Create a mutable context builder
    let mut builder = VMContextBuilder::new();

    // Step 2: Set the signer and account balance directly on the mutable builder
    builder.predecessor_account_id(predecessor);
    builder.account_balance(account_balance);

    // Step 3: Set attached deposit if provided
    if let Some(deposit) = attached_deposit {
        builder.attached_deposit(deposit);
    }

    // Step 4: Return the completed context
    builder.build()
}

pub fn create_valid_liquidity_request(token: AccountId) -> LiquidityRequest {
    LiquidityRequest {
        token,
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
        created_at: 0,
    }
}
