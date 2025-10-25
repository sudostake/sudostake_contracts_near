#![allow(dead_code)]

use crate::{
    contract::{Vault, VaultExt},
    log_event,
};
use near_sdk::{assert_one_yocto, env, near_bindgen, require};

#[near_bindgen]
impl Vault {
    /// Cancels the counter offer submitted by the caller.
    ///
    /// Requirements:
    /// * Exactly 1 yoctoNEAR must be attached (prevents access-key calls).
    /// * A liquidity request must still be open.
    /// * No offer must have been accepted yet.
    /// * The caller must have an active counter offer recorded in the vault.
    #[payable]
    pub fn cancel_counter_offer(&mut self) {
        assert_one_yocto();

        require!(
            self.liquidity_request.is_some(),
            "No liquidity request open"
        );

        require!(
            self.accepted_offer.is_none(),
            "Cannot cancel after offer is accepted"
        );

        self.ensure_processing_idle();

        let caller = env::predecessor_account_id();

        // Remove caller's offer and persist remaining offers (if any).
        let mut offers = self.counter_offers.take().expect("No counter offers found");
        let offer = offers.remove(&caller).expect("No active offer to cancel");
        self.counter_offers = if offers.is_empty() {
            None
        } else {
            Some(offers)
        };

        let request_snapshot = self
            .liquidity_request
            .as_ref()
            .expect("No liquidity request available")
            .clone();
        let token_id = request_snapshot.token.clone();

        // Emit structured event for indexers/clients.
        log_event!(
            "counter_offer_cancelled",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "proposer": caller,
                "amount": offer.amount,
                "request": {
                    "token": token_id.clone(),
                    "amount": request_snapshot.amount,
                    "interest": request_snapshot.interest,
                    "collateral": request_snapshot.collateral,
                    "duration": request_snapshot.duration
                }
            })
        );

        // Kick off refund to return the locked funds to the proposer.
        let _ = self.refund_counter_offer(token_id, offer);
    }
}
