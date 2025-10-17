#![allow(dead_code)]
use crate::{
    contract::Vault,
    types::{ApplyCounterOfferMessage, LiquidityRequest, RefundEntry, APPLY_COUNTER_OFFER_ACTION},
};
use near_sdk::{
    json_types::U128,
    mock::{MockAction, Receipt},
    test_utils::VMContextBuilder,
    AccountId, NearToken,
};

pub const YOCTO_NEAR: u128 = 10u128.pow(24);

pub fn alice() -> AccountId {
    "alice.near".parse().unwrap()
}

pub fn bob() -> AccountId {
    "bob.near".parse().unwrap()
}

pub fn owner() -> AccountId {
    "owner.near".parse().unwrap()
}

pub fn get_context(
    predecessor: AccountId,
    account_balance: NearToken,
    attached_deposit: Option<NearToken>,
) -> near_sdk::VMContext {
    // Create a mutable context builder
    let mut builder = VMContextBuilder::new();

    // Set the signer and account balance directly on the mutable builder
    builder.predecessor_account_id(predecessor);
    builder.account_balance(account_balance);

    // Set attached deposit if provided
    if let Some(deposit) = attached_deposit {
        builder.attached_deposit(deposit);
    }

    // Return the completed context
    builder.build()
}

pub fn get_context_with_timestamp(
    predecessor: AccountId,
    account_balance: NearToken,
    attached_deposit: Option<NearToken>,
    block_timestamp: Option<u64>,
) -> near_sdk::VMContext {
    // Create a mutable context builder
    let mut builder = VMContextBuilder::new();

    // Set signer and balance
    builder.predecessor_account_id(predecessor);
    builder.account_balance(account_balance);
    builder.epoch_height(100);

    // Optional attached deposit
    if let Some(deposit) = attached_deposit {
        builder.attached_deposit(deposit);
    }

    // Optional block timestamp
    if let Some(ts) = block_timestamp {
        builder.block_timestamp(ts);
    }

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

pub fn apply_counter_offer_message_from(request: &LiquidityRequest) -> ApplyCounterOfferMessage {
    ApplyCounterOfferMessage {
        action: APPLY_COUNTER_OFFER_ACTION.to_string(),
        token: request.token.clone(),
        amount: request.amount,
        interest: request.interest,
        collateral: request.collateral,
        duration: request.duration,
    }
}

pub fn apply_counter_offer_msg_string(request: &LiquidityRequest) -> String {
    let message = apply_counter_offer_message_from(request);
    serde_json::to_string(&message).expect("serialize ApplyCounterOfferMessage")
}

/// Inserts a test refund entry into the vault's refund list
pub fn insert_refund_entry(vault: &mut Vault, id: u64, entry: RefundEntry) {
    vault.refund_list.insert(&id, &entry);
}

/// Returns true when any receipt schedules a call to `method`.
pub fn contains_function_call(receipts: &[Receipt], method: &str) -> bool {
    let needle = method.as_bytes();

    receipts
        .iter()
        .flat_map(|receipt| receipt.actions.iter())
        .any(|action| match action {
            MockAction::FunctionCall { method_name, .. } => method_name == needle,
            MockAction::FunctionCallWeight { method_name, .. } => method_name == needle,
            _ => false,
        })
}
