use crate::*;
use near_sdk::collections::Vector;
use near_sdk::{env, AccountId, NearToken};

use crate::contract::Vault;

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
            let mut new_queue = Vector::new(StorageKey::UnstakeEntryPerValidator {
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
}
