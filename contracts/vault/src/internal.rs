use crate::contract::Vault;
use crate::ext::{ext_fungible_token, ext_self, ext_staking_pool};
use crate::log_event;
use crate::types::{
    CounterOffer, RefundEntry, UnstakeEntry, GAS_FOR_FT_TRANSFER, GAS_FOR_VIEW_CALL,
    GAS_FOR_WITHDRAW_ALL, STORAGE_BUFFER,
};
use near_sdk::json_types::U128;
use near_sdk::{env, AccountId, NearToken, Promise};

/// Internal utility methods for Vault
impl Vault {
    pub(crate) fn get_storage_cost(&self) -> u128 {
        let actual_cost = env::storage_byte_cost().as_yoctonear() * env::storage_usage() as u128;
        actual_cost + STORAGE_BUFFER
    }

    pub(crate) fn get_available_balance(&self) -> NearToken {
        let total = env::account_balance().as_yoctonear();
        let available = total.saturating_sub(self.get_storage_cost());
        NearToken::from_yoctonear(available)
    }

    pub(crate) fn get_refund_nonce(&mut self) -> u64 {
        let id = self.refund_nonce;
        self.refund_nonce += 1;

        id
    }

    pub(crate) fn log_gas_checkpoint(&self, method: &str) {
        let gas_left = env::prepaid_gas().as_gas() - env::used_gas().as_gas();
        log_event!(
            "gas_check",
            near_sdk::serde_json::json!({
                "method": method,
                "gas_left": gas_left
            })
        );
    }

    pub(crate) fn refund_all_counter_offers(&self, token: AccountId) {
        if let Some(counter_offers) = &self.counter_offers {
            for (_, offer) in counter_offers.iter() {
                self.refund_counter_offer(token.clone(), offer);
            }
        }
    }

    pub(crate) fn refund_counter_offer(&self, token_address: AccountId, offer: CounterOffer) {
        ext_fungible_token::ext(token_address.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .ft_transfer(offer.proposer.clone(), offer.amount, None)
            .then(ext_self::ext(env::current_account_id()).on_refund_complete(
                offer.proposer.clone(),
                offer.amount,
                token_address,
            ));
    }

    pub(crate) fn batch_claim_unstaked(
        &self,
        validators: Vec<AccountId>,
        call_back: Promise,
    ) -> Promise {
        // Start the batch chain with the first validator
        let mut chain = ext_staking_pool::ext(validators[0].clone())
            .with_static_gas(GAS_FOR_WITHDRAW_ALL)
            .withdraw_all();

        // Fold the remaining validators into the chain
        for validator in validators.iter().skip(1) {
            chain = chain.and(
                ext_staking_pool::ext(validator.clone())
                    .with_static_gas(GAS_FOR_WITHDRAW_ALL)
                    .withdraw_all(),
            );
        }

        // Add the final callback to handle results
        chain.then(call_back)
    }

    pub(crate) fn batch_query_total_staked(&self, call_back: Promise) -> Promise {
        // Ensure there are validators to query
        let mut validators = self.active_validators.iter();
        let first = validators
            .next()
            .expect("No active validators available for collateral check");

        // Start staking_pool view call chain
        let initial = ext_staking_pool::ext(first.clone())
            .with_static_gas(GAS_FOR_VIEW_CALL)
            .get_account_staked_balance(env::current_account_id());

        // Fold the remaining instructions into the clain
        let chain = validators.fold(initial, |acc, validator| {
            acc.and(
                ext_staking_pool::ext(validator.clone())
                    .with_static_gas(GAS_FOR_VIEW_CALL)
                    .get_account_staked_balance(env::current_account_id()),
            )
        });

        // Add the final callback to handle results
        chain.then(call_back)
    }

    pub(crate) fn batch_unstake(
        &self,
        unstake_instructions: Vec<(AccountId, u128)>,
        call_back: Promise,
    ) -> Promise {
        // Build the batch unstake call chain
        let mut chain = ext_staking_pool::ext(unstake_instructions[0].0.clone())
            .with_static_gas(crate::types::GAS_FOR_UNSTAKE)
            .unstake(U128::from(unstake_instructions[0].1));

        // Fold the remaining instructions into the chain
        for (validator, amount) in unstake_instructions.iter().skip(1) {
            chain = chain.and(
                ext_staking_pool::ext(validator.clone())
                    .with_static_gas(crate::types::GAS_FOR_UNSTAKE)
                    .unstake(U128::from(*amount)),
            );
        }

        // Add the final callback to handle results
        chain.then(call_back)
    }

    pub(crate) fn add_refund_entry(
        &mut self,
        token: Option<AccountId>,
        proposer: AccountId,
        amount: U128,
        refund_id: Option<u64>,
    ) {
        let id = refund_id.unwrap_or_else(|| self.get_refund_nonce());
        self.refund_list.insert(
            &id,
            &RefundEntry {
                token,
                proposer,
                amount,
                added_at_epoch: env::epoch_height(),
            },
        );
    }

    pub(crate) fn update_validator_unstake_entry(&mut self, validator: &AccountId, amount: u128) {
        // Get the validator unstake entry
        let mut entry = self
            .unstake_entries
            .get(&validator)
            .unwrap_or_else(|| UnstakeEntry {
                amount: 0,
                epoch_height: 0,
            });

        // Update the entry and save to state
        entry.amount += amount;
        entry.epoch_height = env::epoch_height();
        self.unstake_entries.insert(&validator, &entry);
    }
}
