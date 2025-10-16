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

        let caller = env::predecessor_account_id();
        // Counter offers must exist and the caller must have an entry.
        let mut emptied = false;
        let offer = {
            let offers_map = self
                .counter_offers
                .as_mut()
                .expect("No counter offers found");
            let offer = offers_map
                .remove(&caller)
                .expect("No active offer to cancel");

            if offers_map.is_empty() {
                // Explicitly clear storage of counter offers when map becomes empty.
                offers_map.clear();
                emptied = true;
            }

            offer
        };

        if emptied {
            self.counter_offers = None;
        }

        let (token_id, amount, interest, collateral, duration) = {
            let liquidity_request = self
                .liquidity_request
                .as_ref()
                .expect("No liquidity request available");
            (
                liquidity_request.token.clone(),
                liquidity_request.amount,
                liquidity_request.interest,
                liquidity_request.collateral.clone(),
                liquidity_request.duration,
            )
        };

        // Log counter_offer_cancelled event
        log_event!(
            "counter_offer_cancelled",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "proposer": caller,
                "amount": offer.amount,
                "request": {
                    "token": token_id.clone(),
                    "amount": amount,
                    "interest": interest,
                    "collateral": collateral,
                    "duration": duration
                }
            })
        );

        // Attempt refund
        let _ = self.refund_counter_offer(token_id, offer);
    }
}
