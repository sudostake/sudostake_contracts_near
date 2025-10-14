use crate::contract::Vault;

use crate::log_event;
use crate::types::{CounterOffer, CounterOfferMessage, StorageKey, MAX_COUNTER_OFFERS};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::{env, require, AccountId};

impl Vault {
    pub fn try_add_counter_offer(
        &mut self,
        proposer: AccountId,
        offered_amount: U128,
        msg: CounterOfferMessage,
        token_contract: AccountId,
    ) -> Result<(), String> {
        // Must have a liquidity request
        let request = self
            .liquidity_request
            .as_ref()
            .ok_or("No liquidity request available")?;

        // Get request details
        let r_token = request.token.clone();
        let r_amount = request.amount;
        let r_interest = request.interest;
        let r_collateral = request.collateral;
        let r_duration = request.duration;

        // Request must not be already accepted
        require!(
            self.accepted_offer.is_none(),
            "Liquidity request already accepted"
        );

        // Calling token_contract must match requested token
        require!(token_contract == r_token, "Token mismatch");

        // Ensure message fields match the current request
        require!(
            msg.token == r_token
                && msg.amount == r_amount
                && msg.interest == r_interest
                && msg.collateral == r_collateral
                && msg.duration == r_duration,
            "Message fields do not match current request"
        );

        // Offered amount must be > 0
        require!(
            offered_amount > U128::from(0),
            "Offer amount must be greater than 0"
        );

        // Offered amount must be < requested amount
        require!(
            offered_amount < r_amount,
            "Offer must be less than requested amount"
        );

        // Get or initialize counter_offers map if needed
        let mut offers_map = self
            .counter_offers
            .take()
            .unwrap_or_else(|| UnorderedMap::new(StorageKey::CounterOffers));

        // Proposer must not already have an offer
        require!(
            offers_map.get(&proposer).is_none(),
            "Proposer already has an active offer"
        );

        // Find current best & worst offers simultaneously
        let mut best = U128::from(0);
        let mut worst: Option<(AccountId, CounterOffer)> = None;
        let evict_worse_offfer = (offers_map.len() + 1) > MAX_COUNTER_OFFERS;
        for (k, v) in offers_map.iter() {
            let amt = v.amount;
            if amt > best {
                best = amt;
            }

            if evict_worse_offfer {
                match &worst {
                    None => worst = Some((k, v)),
                    Some((_, w)) if amt < w.amount => worst = Some((k, v)),
                    _ => {}
                }
            }
        }

        // Offer must be better than the current best
        require!(
            offered_amount > best,
            "Offer must be greater than current best offer"
        );

        // Add the offer to the list of counter offers
        offers_map.insert(
            &proposer,
            &CounterOffer {
                proposer: proposer.clone(),
                amount: offered_amount,
                timestamp: env::block_timestamp(),
            },
        );

        let mut evicted_offer: Option<CounterOffer> = None;

        // Log counter_offer_created event
        log_event!(
            "counter_offer_created",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "proposer": proposer,
                "amount": offered_amount,
                "request": {
                    "token": r_token,
                    "amount": r_amount,
                    "interest": r_interest,
                    "collateral": r_collateral,
                    "duration": r_duration
                }
            })
        );

        // Try evict worst offer
        if evict_worse_offfer {
            if let Some((lowest_key, lowest_offer)) = worst {
                offers_map.remove(&lowest_key);

                // Log counter_offer_evicted event
                log_event!(
                    "counter_offer_evicted",
                    near_sdk::serde_json::json!({
                        "vault": env::current_account_id(),
                        "proposer": lowest_key,
                        "amount": lowest_offer.amount,
                        "request": {
                            "token": r_token,
                            "amount": r_amount,
                            "interest": r_interest,
                            "collateral": r_collateral,
                            "duration": r_duration
                        }
                    })
                );

                evicted_offer = Some(lowest_offer);
            }
        }

        if offers_map.is_empty() {
            offers_map.clear();
            self.counter_offers = None;
        } else {
            self.counter_offers = Some(offers_map);
        }

        if let Some(lowest_offer) = evicted_offer {
            let _ = self.refund_counter_offer(token_contract, lowest_offer);
        }

        Ok(())
    }
}
