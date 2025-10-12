#![allow(dead_code)]

use crate::{
    contract::{Vault, VaultExt},
    log_event,
};
use near_sdk::{assert_one_yocto, env, near_bindgen, require};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn cancel_counter_offer(&mut self) {
        // Require 1 yoctoNEAR for access control
        assert_one_yocto();

        // Vault must have an open liquidity request
        require!(
            self.liquidity_request.is_some(),
            "No liquidity request open"
        );

        // Request must not have been accepted
        require!(
            self.accepted_offer.is_none(),
            "Cannot cancel after offer is accepted"
        );

        // Counter offers must exist
        let mut offers_map = self.counter_offers.take().expect("No counter offers found");

        // Caller must have an active counter offer
        let caller = env::predecessor_account_id();
        let offer = offers_map
            .remove(&caller)
            .expect("No active offer to cancel");

        // Reset counter_offers to None when empty
        if offers_map.is_empty() {
            // Explicitly clear storage of counter offers when map becomes empty.
            offers_map.clear();
            self.counter_offers = None;
        } else {
            self.counter_offers = Some(offers_map);
        }

        // Log counter_offer_cancelled event
        let liquidity_request = self.liquidity_request.as_ref().unwrap();
        log_event!(
            "counter_offer_cancelled",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "proposer": caller,
                "amount": offer.amount,
                "request": {
                    "token": liquidity_request.token,
                    "amount": liquidity_request.amount,
                    "interest": liquidity_request.interest,
                    "collateral": liquidity_request.collateral,
                    "duration": liquidity_request.duration
                }
            })
        );

        // Attempt refund
        let _ = self.refund_counter_offer(liquidity_request.token.clone(), offer);
    }
}
