use crate::contract::Vault;
use crate::log_event;
use crate::types::AcceptRequestMessage;

use near_sdk::json_types::U128;
use near_sdk::{env, AccountId};

impl Vault {
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
        self.refund_all_counter_offers(msg.token.clone());

        // Log liquidity_request_accepted event
        log_event!(
            "liquidity_request_accepted",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "lender": lender,
                "amount": amount.0.to_string(),
                "timestamp": env::block_timestamp()
            })
        );

        Ok(())
    }
}
