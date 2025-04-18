#![allow(dead_code)]

use crate::{
    contract::{Vault, VaultExt},
    log_event,
};
use near_sdk::{assert_one_yocto, env, json_types::U128, near_bindgen, require, AccountId};

#[near_bindgen]
impl Vault {
    /// Accepts a counter offer identified by `proposer_id` with their proposed `amount`.
    /// This function can only be called by the vault owner.
    /// It validates the offer, marks it as accepted, refunds all other counter offers,
    /// and clears the counter offers storage.
    #[payable]
    pub fn accept_counter_offer(&mut self, proposer_id: AccountId, amount: U128) {
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

        // Retrieve the specific counter offer for the given proposer_id.
        let offer = offers_map
            .remove(&proposer_id)
            .expect("Counter offer from proposer not found");

        // Ensure the given amount matches the stored counter offer amount.
        require!(
            offer.amount == amount,
            "Provided amount does not match the counter offer"
        );

        // Record acceptance
        self.accepted_offer = Some(crate::types::AcceptedOffer {
            lender: proposer_id.clone(),
            accepted_at: env::block_timestamp(),
        });

        // Refund all other counter offers
        let token = liquidity_request.token.clone();
        for other_offer in offers_map.values() {
            self.refund_counter_offer(token.clone(), other_offer);
        }

        // Log the counter offer acceptance event.
        log_event!(
            "counter_offer_accepted",
            near_sdk::serde_json::json!({
                "accepted_proposer": proposer_id,
                "accepted_amount": amount.0.to_string(),
                "timestamp": env::block_timestamp(),
                "request": liquidity_request
            })
        );
    }
}
