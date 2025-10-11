use near_sdk::{json_types::U128, testing_env, NearToken, PromiseOrValue};
use serde_json::json;

use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;

use crate::contract::Vault;

use super::test_utils::{alice, get_context, owner};

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
