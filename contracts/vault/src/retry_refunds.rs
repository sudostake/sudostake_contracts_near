#![allow(dead_code)]

use near_sdk::json_types::U128;
use near_sdk::{assert_one_yocto, require, AccountId, NearToken};
use near_sdk::{env, near_bindgen};

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::ext::ext_fungible_token;
use crate::types::{RefundEntry, GAS_FOR_FT_TRANSFER};

#[near_bindgen]
impl Vault {
    #[private]
    pub fn on_refund_complete(
        &mut self,
        proposer: AccountId,
        amount: U128,
        token_address: AccountId,
    ) {
        self.log_gas_checkpoint("on_refund_complete");

        match env::promise_result(0) {
            near_sdk::PromiseResult::Successful(_) => {
                // refund succeeded — do nothing
            }
            _ => {
                env::log_str(&format!(
                    "Refund failed for proposer {}, amount {}, token_address {}",
                    proposer, amount.0, token_address
                ));

                // Add to refund_list for retry
                let id = self.refund_nonce;
                self.refund_list.insert(
                    &id,
                    &RefundEntry {
                        token: token_address,
                        proposer,
                        amount,
                    },
                );
                self.refund_nonce += 1;
            }
        }
    }

    #[payable]
    pub fn retry_refunds(&mut self) {
        // Require 1 yoctoNEAR for access control
        assert_one_yocto();

        // Collect refund entries to retry
        let caller = env::predecessor_account_id();
        let mut to_retry: Vec<(u64, RefundEntry)> = vec![];
        for (id, entry) in self.refund_list.iter() {
            if caller == self.owner || caller == entry.proposer {
                to_retry.push((id, entry));
            }
        }

        // to_rety list must not be empty
        require!(
            !to_retry.is_empty(),
            "No refundable entries found for caller"
        );

        // Try to refund all entries on the to_retry list
        for (id, entry) in to_retry {
            ext_fungible_token::ext(entry.token.clone())
                .with_attached_deposit(NearToken::from_yoctonear(1))
                .with_static_gas(GAS_FOR_FT_TRANSFER)
                .ft_transfer(entry.proposer.clone(), entry.amount, None)
                .then(Self::ext(env::current_account_id()).on_retry_refund_complete(id));
        }
    }
    #[private]
    pub fn on_retry_refund_complete(&mut self, id: u64) {
        self.log_gas_checkpoint("on_retry_refund_complete");

        match env::promise_result(0) {
            near_sdk::PromiseResult::Successful(_) => {
                self.refund_list.remove(&id);
                env::log_str(&format!("Retry refund succeeded and removed for ID {}", id));
            }
            _ => {
                env::log_str(&format!("Retry refund failed again for ID {}", id));
            }
        }
    }
}
