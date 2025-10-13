#![allow(dead_code)]

use crate::{
    contract::{Vault, VaultExt},
    log_event,
};

use near_sdk::{
    assert_one_yocto, env, json_types::U128, near_bindgen, require, AccountId, PromiseOrValue,
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
        assert_one_yocto();

        let caller = env::predecessor_account_id();
        require!(
            caller == self.owner,
            "Only the vault owner can accept a counter offer"
        );

        let token = self
            .liquidity_request
            .as_ref()
            .map(|request| request.token.clone())
            .expect("No liquidity request available");

        require!(
            self.accepted_offer.is_none(),
            "Liquidity request already accepted"
        );

        let mut offers = self
            .counter_offers
            .take()
            .expect("No counter offers available");

        let offer = offers
            .remove(&proposer_id)
            .expect("Counter offer from proposer not found");

        require!(
            offer.amount == amount,
            "Provided amount does not match the counter offer"
        );

        let accepted_amount = offer.amount;

        self.accepted_offer = Some(crate::types::AcceptedOffer {
            lender: proposer_id.clone(),
            accepted_at: env::block_timestamp(),
        });

        let liquidity_request = {
            let request = self
                .liquidity_request
                .as_mut()
                .expect("No liquidity request available");
            request.amount = accepted_amount;
            request.clone()
        };

        for other_offer in offers.values() {
            let _ = self.refund_counter_offer(token.clone(), other_offer);
        }

        offers.clear();
        self.counter_offers = None;

        log_event!(
            "counter_offer_accepted",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "accepted_proposer": proposer_id,
                "accepted_amount": accepted_amount.0.to_string(),
                "timestamp": env::block_timestamp(),
                "request": liquidity_request
            })
        );

        PromiseOrValue::Value(())
    }
}
