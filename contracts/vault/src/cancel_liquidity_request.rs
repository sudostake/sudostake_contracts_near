#![allow(dead_code)]

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::log_event;
use near_sdk::{assert_one_yocto, env, near_bindgen, require};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn cancel_liquidity_request(&mut self) {
        // Require 1 yoctoNEAR for access control
        assert_one_yocto();

        // Must be the vault owner
        require!(
            env::predecessor_account_id() == self.owner,
            "Only the vault owner can cancel the liquidity request"
        );

        // Must have an active liquidity request
        let request = self
            .liquidity_request
            .as_ref()
            .expect("No active liquidity request");

        // Cannot cancel after an offer has been accepted
        require!(
            self.accepted_offer.is_none(),
            "Cannot cancel after an offer has been accepted"
        );

        // Refund all counter offers
        self.refund_all_counter_offers(request.token.clone());

        // Clean up state
        self.liquidity_request = None;
        self.counter_offers = None;

        // Emit liquidity_request_cancelled event
        log_event!(
            "liquidity_request_cancelled",
            near_sdk::serde_json::json!({
               "vault": env::current_account_id()
            })
        );
    }
}
