use crate::contract::Vault;
use crate::ext::ext_fungible_token;
use crate::log_event;
use crate::types::{AcceptRequestMessage, GAS_FOR_FT_TRANSFER, STORAGE_BUFFER};
use near_sdk::json_types::U128;
use near_sdk::{env, AccountId, NearToken};

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

    pub(crate) fn refund_all_counter_offers(&self, token: AccountId) {
        if let Some(counter_offers) = &self.counter_offers {
            for (_, offer) in counter_offers.iter() {
                ext_fungible_token::ext(token.clone())
                    .with_attached_deposit(NearToken::from_yoctonear(1))
                    .with_static_gas(GAS_FOR_FT_TRANSFER)
                    .ft_transfer(offer.proposer.clone(), offer.amount, None)
                    .then(Self::ext(env::current_account_id()).on_refund_complete(
                        offer.proposer.clone(),
                        offer.amount,
                        token.clone(),
                    ));
            }
        }
    }
}
