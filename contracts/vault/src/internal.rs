use crate::contract::Vault;
use crate::log_event;
use crate::types::{AcceptRequestMessage, StorageKey, STORAGE_BUFFER};
use near_sdk::collections::Vector;
use near_sdk::json_types::U128;
use near_sdk::{env, AccountId, NearToken};

/// Internal utility methods for Vault
impl Vault {
    pub fn reconcile_after_withdraw(&mut self, validator: &AccountId, remaining: NearToken) {
        let total_before = self.total_unstaked(validator);
        let withdrawn = total_before
            .as_yoctonear()
            .saturating_sub(remaining.as_yoctonear());
        self.reconcile_unstake_entries(validator, withdrawn);
    }

    pub fn total_unstaked(&self, validator: &AccountId) -> NearToken {
        self.unstake_entries
            .get(validator)
            .map(|queue| queue.iter().map(|entry| entry.amount).sum::<u128>())
            .map(NearToken::from_yoctonear)
            .unwrap_or_else(|| NearToken::from_yoctonear(0))
    }

    pub fn reconcile_unstake_entries(&mut self, validator: &AccountId, mut withdrawn: u128) {
        if let Some(queue) = self.unstake_entries.get(validator) {
            let mut new_queue = Vector::new(StorageKey::UnstakeEntriesPerValidator {
                validator_hash: env::sha256(validator.as_bytes()),
            });

            for entry in queue.iter() {
                if withdrawn >= entry.amount {
                    withdrawn = withdrawn.saturating_sub(entry.amount);
                } else {
                    new_queue.push(&entry);
                }
            }

            if new_queue.is_empty() {
                self.unstake_entries.remove(validator);
            } else {
                self.unstake_entries.insert(validator, &new_queue);
            }
        }
    }

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
}
