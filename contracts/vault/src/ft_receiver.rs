use crate::contract::VaultExt;
use crate::Vault;
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
        env::log_str(&format!(
            "Received {} tokens from {} with message: {}",
            amount.0, sender_id, msg
        ));

        PromiseOrValue::Value(U128(0))
    }
}
