use crate::{
    contract::{Vault, VaultExt},
    log_event,
};
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::{json_types::U128, near_bindgen, AccountId, PromiseOrValue};

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
                "sender": sender_id,
                "amount": amount.0.to_string(),
                "msg": msg
            })
        );

        PromiseOrValue::Value(U128(0))
    }
}
