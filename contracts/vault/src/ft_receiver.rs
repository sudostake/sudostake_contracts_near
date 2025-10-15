use crate::{
    contract::{Vault, VaultExt},
    log_event,
    types::{ApplyCounterOfferMessage, APPLY_COUNTER_OFFER_ACTION},
};
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::{env, json_types::U128, near_bindgen, AccountId, PromiseOrValue};

#[near_bindgen]
impl FungibleTokenReceiver for Vault {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        log_event!(
            "ft_on_transfer",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "sender": sender_id,
                "amount": amount.0.to_string(),
                "msg": msg
            })
        );

        // Try ApplyCounterOffer
        if let Ok(parsed) = near_sdk::serde_json::from_str::<ApplyCounterOfferMessage>(&msg) {
            if parsed.action == APPLY_COUNTER_OFFER_ACTION {
                let is_direct_accept = self
                    .liquidity_request
                    .as_ref()
                    .map(|request| request.amount == amount)
                    .unwrap_or(false);

                if is_direct_accept {
                    let result = self.try_accept_liquidity_request(
                        sender_id.clone(),
                        amount,
                        parsed,
                        env::predecessor_account_id(),
                    );

                    return match result {
                        Ok(_) => PromiseOrValue::Value(U128(0)),
                        Err(_) => PromiseOrValue::Value(amount),
                    };
                } else {
                    let result = self.try_add_counter_offer(
                        sender_id.clone(),
                        amount,
                        parsed,
                        env::predecessor_account_id(),
                    );

                    return match result {
                        Ok(_) => PromiseOrValue::Value(U128(0)),
                        Err(_) => PromiseOrValue::Value(amount),
                    };
                }
            }
        }

        // Invalid or unknown message — keep tokens but ignore
        PromiseOrValue::Value(amount)
    }
}
