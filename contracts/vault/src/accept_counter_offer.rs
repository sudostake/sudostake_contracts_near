#![allow(dead_code)]

use crate::{
    contract::{Vault, VaultExt},
    ext::ext_self,
    log_event,
    types::{GAS_FOR_CALLBACK, GAS_FOR_FT_TRANSFER},
};
use near_sdk::{
    assert_one_yocto, env, json_types::U128, near_bindgen, require, AccountId, NearToken, Promise,
    PromiseOrValue,
};

#[near_bindgen]
impl Vault {
    /// Accepts a counter offer identified by `proposer_id` with their proposed `amount`.
    /// This function can only be called by the vault owner.
    /// It validates the offer, marks it as accepted, refunds all other counter offers,
    /// and clears the counter offers storage.
    #[payable]
    pub fn accept_counter_offer(
        &mut self,
        proposer_id: AccountId,
        amount: U128,
    ) -> PromiseOrValue<()> {
        // Require 1 yoctoNEAR for access control
        assert_one_yocto();

        // Only the vault owner can perform this action
        require!(
            env::predecessor_account_id() == self.owner,
            "Only the vault owner can accept a counter offer"
        );

        // Ensure there is an active liquidity request.
        let liquidity_request = self
            .liquidity_request
            .as_ref()
            .expect("No liquidity request available");

        // Ensure the liquidity request has not yet been accepted.
        require!(
            self.accepted_offer.is_none(),
            "Liquidity request already accepted"
        );

        // Ensure there are existing counter offers.
        let mut offers_map = self
            .counter_offers
            .take()
            .expect("No counter offers available");

        // Retrieve and remove the specific counter offer for the given proposer_id.
        let offer = offers_map
            .remove(&proposer_id)
            .expect("Counter offer from proposer not found");

        let token = liquidity_request.token.clone();

        // Ensure the given amount matches the stored counter offer amount.
        if offer.amount != amount {
            if offers_map.is_empty() {
                offers_map.clear();
                self.counter_offers = None;
            } else {
                self.counter_offers = Some(offers_map);
            }

            let proposer = offer.proposer.clone();
            let amount = offer.amount;
            let transfer_args = near_sdk::serde_json::to_vec(&near_sdk::serde_json::json!({
                "receiver_id": proposer.clone(),
                "amount": amount,
                "memo": Option::<String>::None
            }))
            .expect("Failed to serialize ft_transfer arguments");

            let refund_promise = Promise::new(token.clone())
                .function_call(
                    "ft_transfer".to_string(),
                    transfer_args,
                    NearToken::from_yoctonear(1),
                    GAS_FOR_FT_TRANSFER,
                )
                .then(ext_self::ext(env::current_account_id()).on_refund_complete(
                    proposer.clone(),
                    amount,
                    token.clone(),
                ));
            let panic_promise = ext_self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_CALLBACK)
                .on_accept_counter_offer_mismatch_fail();

            drop(refund_promise);
            return PromiseOrValue::Promise(panic_promise.as_return());
        }

        // Record acceptance
        self.accepted_offer = Some(crate::types::AcceptedOffer {
            lender: proposer_id.clone(),
            accepted_at: env::block_timestamp(),
        });

        // Refund all other counter offers
        for other_offer in offers_map.values() {
            let _ = self.refund_counter_offer(token.clone(), other_offer);
        }

        // Remove all counter offers from storage now that refunds are initiated.
        offers_map.clear();

        // Log the counter offer acceptance event.
        log_event!(
            "counter_offer_accepted",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "accepted_proposer": proposer_id,
                "accepted_amount": amount.0.to_string(),
                "timestamp": env::block_timestamp(),
                "request": liquidity_request
            })
        );

        PromiseOrValue::Value(())
    }

    #[private]
    pub fn on_accept_counter_offer_mismatch_fail(&mut self) {
        env::panic_str("Provided amount does not match the counter offer");
    }
}
