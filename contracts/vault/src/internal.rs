use crate::contract::Vault;
use crate::ext::ext_fungible_token;
use crate::log_event;
use crate::types::{
    AcceptRequestMessage, CounterOffer, CounterOfferMessage, StorageKey, GAS_FOR_FT_TRANSFER,
    MAX_COUNTER_OFFERS, STORAGE_BUFFER,
};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::{env, require, AccountId, NearToken};

/// Internal utility methods for Vault
impl Vault {
    pub fn get_available_balance(&self) -> NearToken {
        let total = env::account_balance().as_yoctonear();
        let available = total.saturating_sub(STORAGE_BUFFER);
        NearToken::from_yoctonear(available)
    }

    pub fn log_gas_checkpoint(&self, method: &str) {
        let gas_left = env::prepaid_gas().as_gas() - env::used_gas().as_gas();
        log_event!(
            "gas_check",
            near_sdk::serde_json::json!({
                "method": method,
                "gas_left": gas_left
            })
        );
    }

    pub fn try_accept_liquidity_request(
        &mut self,
        lender: AccountId,
        amount: U128,
        msg: AcceptRequestMessage,
        token_contract: AccountId,
    ) -> Result<(), String> {
        // Must have a liquidity request
        let request = self
            .liquidity_request
            .as_ref()
            .ok_or("No liquidity request available")?;

        // Must not be already fulfilled
        if self.accepted_offer.is_some() {
            return Err("Liquidity request already fulfilled".into());
        }

        // Sender must not be the vault owner
        if lender == self.owner {
            return Err("Vault owner cannot fulfill their own request".into());
        }

        // Token must match
        if msg.token != request.token || token_contract != request.token {
            return Err("Token mismatch".into());
        }

        // Amount must match
        if msg.amount != request.amount || amount != request.amount {
            return Err("Amount mismatch".into());
        }

        // Interest must match
        if msg.interest != request.interest {
            return Err("Interest mismatch".into());
        }

        // Collateral must match
        if msg.collateral != request.collateral {
            return Err("Collateral mismatch".into());
        }

        // Duration must match
        if msg.duration != request.duration {
            return Err("Duration mismatch".into());
        }

        // Save accepted offer
        self.accepted_offer = Some(crate::types::AcceptedOffer {
            lender: lender.clone(),
            accepted_at: env::block_timestamp(),
        });

        // Refund all counter offers if any
        self.refund_all_counter_offers(msg.token);

        // Log liquidity_request_accepted event
        log_event!(
            "liquidity_request_accepted",
            near_sdk::serde_json::json!({
                "lender": lender,
                "amount": amount.0.to_string(),
                "timestamp": env::block_timestamp()
            })
        );

        Ok(())
    }

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
        let offers_map = self
            .counter_offers
            .get_or_insert_with(|| UnorderedMap::new(StorageKey::CounterOffers));

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

        // Log counter_offer_created event
        log_event!(
            "counter_offer_created",
            near_sdk::serde_json::json!({
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

                // Refund lowest_offer
                self.refund_counter_offer(token_contract, lowest_offer);
            }
        }

        Ok(())
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
            .then(Self::ext(env::current_account_id()).on_refund_complete(
                offer.proposer.clone(),
                offer.amount,
                token_address,
            ));
    }
}
