use crate::contract::VaultExt;
use crate::ext_self;
use crate::log_event;
use crate::Vault;
use crate::GAS_FOR_FT_TRANSFER;
use near_sdk::NearToken;
use near_sdk::{assert_one_yocto, env, json_types::U128, near_bindgen, AccountId, Promise};

#[near_bindgen]
impl Vault {
    /// Withdraw NEAR or NEP-141 token from the vault.
    /// - `token_address = None` means withdraw NEAR
    /// - `token_address = Some(token)` means withdraw NEP-141
    /// - `to` is the recipient (defaults to vault owner)
    #[payable]
    pub fn withdraw_balance(
        &mut self,
        token_address: Option<AccountId>,
        amount: U128,
        to: Option<AccountId>,
    ) -> Promise {
        // Ensure that only the vault owner can call this method
        let caller = env::predecessor_account_id();
        assert_eq!(caller, self.owner, "Only the vault owner can withdraw");

        // Determine the recipient of the withdrawal
        let recipient = to.unwrap_or_else(|| self.owner.clone());

        // If no token address is provided, perform a NEAR withdrawal
        if token_address.is_none() {
            let amount = NearToken::from_yoctonear(amount.0);
            return self.internal_withdraw_near(amount, recipient);
        }

        // A NEP-141 token withdrawal is requested — require 1 yoctoNEAR
        assert_one_yocto();

        // Extract the token contract address
        let token = token_address.unwrap();

        // Emit withdraw_ft event
        log_event!(
            "withdraw_ft",
            near_sdk::serde_json::json!({
                "token": token,
                "to": recipient,
                "amount": amount.0.to_string()
            })
        );

        // Call `ft_transfer` on the token contract to send tokens to recipient
        ext_self::ext(token)
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .ft_transfer(recipient, amount, None)
    }

    /// Internal helper method to withdraw NEAR from the vault
    fn internal_withdraw_near(&self, amount: NearToken, recipient: AccountId) -> Promise {
        // Retrieve the available NEAR balance (subtracting storage buffer)
        let available = self.get_available_balance();

        // Ensure the requested amount does not exceed what's safely withdrawable
        assert!(
            amount <= available,
            "Not enough NEAR balance. Available: {}, Requested: {}",
            available.as_yoctonear(),
            amount.as_yoctonear()
        );

        // Emit structured withdraw_near event using macro
        log_event!(
            "withdraw_near",
            near_sdk::serde_json::json!({
                "to": recipient,
                "amount": amount.as_yoctonear().to_string()
            })
        );

        // Return a Promise to transfer the NEAR to the recipient
        Promise::new(recipient).transfer(amount)
    }
}
