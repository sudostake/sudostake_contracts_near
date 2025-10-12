use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::ext::ext_self;
use crate::ext::ext_staking_pool;
use crate::log_event;
use crate::types::ProcessingState;
use crate::types::MAX_ACTIVE_VALIDATORS;
use crate::types::{GAS_FOR_CALLBACK, GAS_FOR_DEPOSIT_AND_STAKE};
use near_sdk::{assert_one_yocto, env, near_bindgen, require, AccountId, NearToken, Promise};

#[near_bindgen]
impl Vault {
    /// Stakes `amount` of NEAR to the given validator on behalf of the vault owner.
    #[payable]
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn delegate(&mut self, validator: AccountId, amount: NearToken) -> Promise {
        assert_one_yocto();

        let caller = env::predecessor_account_id();
        require!(amount.as_yoctonear() > 0, "Amount must be greater than 0");
        require!(
            caller == self.owner,
            "Only the vault owner can delegate stake"
        );

        let available = self.get_available_balance();
        require!(
            amount <= available,
            format!(
                "Requested amount ({}) exceeds available balance ({})",
                amount.as_yoctonear(),
                available.as_yoctonear()
            )
        );

        let validator_is_new = !self.active_validators.contains(&validator);
        if validator_is_new {
            require!(
                self.active_validators.len() < MAX_ACTIVE_VALIDATORS,
                format!(
                    "You can only stake with {} validators at a time",
                    MAX_ACTIVE_VALIDATORS
                ),
            );
        }

        require!(
            self.refund_list.is_empty(),
            "Cannot delegate while there are pending refund entries"
        );
        require!(
            self.liquidation.is_none(),
            "Cannot delegate while liquidation is in progress"
        );

        self.acquire_processing_lock(ProcessingState::Delegate);

        ext_staking_pool::ext(validator.clone())
            .with_static_gas(GAS_FOR_DEPOSIT_AND_STAKE)
            .with_attached_deposit(amount)
            .deposit_and_stake()
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_deposit_and_stake(validator, amount),
            )
    }

    #[private]
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn on_deposit_and_stake(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) {
        self.log_gas_checkpoint("on_deposit_and_stake");
        self.release_processing_lock();

        match result {
            Ok(()) => {
                self.active_validators.insert(&validator);
                log_event!(
                    "delegate_completed",
                    near_sdk::serde_json::json!({
                        "vault": env::current_account_id(),
                        "validator": validator,
                        "amount": amount
                    })
                );
            }
            Err(_) => {
                log_event!(
                    "delegate_failed",
                    near_sdk::serde_json::json!({
                        "vault": env::current_account_id(),
                        "validator": validator,
                        "amount": amount,
                        "error": "deposit_and_stake failed"
                    })
                );
            }
        }
    }
}
