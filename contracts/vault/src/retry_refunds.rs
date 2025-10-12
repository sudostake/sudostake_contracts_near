#![allow(dead_code)]

use near_sdk::{
    assert_one_yocto, env, json_types::U128, near_bindgen, require, AccountId, NearToken, Promise,
};

use crate::{
    contract::{Vault, VaultExt},
    ext::ext_fungible_token,
    log_event,
    types::{RefundEntry, GAS_FOR_FT_TRANSFER, REFUND_EXPIRY_EPOCHS},
};

/// Convenient alias for the refund identifier.
type RefundId = u64;

#[near_bindgen]
impl Vault {
    /// Callback fired after the *initial* refund attempt.
    ///
    /// If the transfer failed we persist the entry so that it can be retried
    /// later via [`retry_refunds`].
    #[private]
    pub fn on_refund_complete(
        &mut self,
        proposer: AccountId,
        amount: U128,
        token_address: AccountId,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) {
        self.log_gas_checkpoint("on_refund_complete");

        if result.is_ok() {
            // 👌 Nothing more to do – the refund was successful.
            return;
        }

        log_event!(
            "refund_failed",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "proposer": proposer,
                "amount": amount,
                "token": token_address
            })
        );

        self.add_refund_entry(Some(token_address), proposer, amount, None);
    }

    /// Manually retries refunds that previously failed.
    ///
    /// - **Access control** – requires `1 yoctoⓃ` and can be called by the
    ///   contract owner or by the original proposer whose refund failed.
    #[payable]
    pub fn retry_refunds(&mut self) {
        assert_one_yocto();

        let caller = env::predecessor_account_id();
        let mut to_retry: Vec<(RefundId, RefundEntry)> = self
            .refund_list
            .iter()
            .filter(|(_, entry)| caller == self.owner || caller == entry.proposer)
            .collect();

        require!(
            !to_retry.is_empty(),
            "No refundable entries found for caller"
        );

        let current_epoch = env::epoch_height();

        // Move matching entries into a temporary `to_retry` vector so the immutable
        // borrow on `refund_list` ends before we call `schedule_refund`, which may
        // mutate contract state (including `refund_list`).
        for (id, entry) in to_retry.drain(..) {
            self.refund_list.remove(&id);

            // Skip expired entries instead of scheduling another transfer.
            if current_epoch >= entry.added_at_epoch.saturating_add(REFUND_EXPIRY_EPOCHS) {
                log_event!(
                    "retry_refund_expired",
                    near_sdk::serde_json::json!({
                        "vault": env::current_account_id(),
                        "refund_id": id,
                        "proposer": entry.proposer
                    })
                );
                continue;
            }

            self.schedule_refund(id, entry);
        }
    }

    /// Schedules a refund promise and attaches the unified callback.
    fn schedule_refund(&self, id: RefundId, entry: RefundEntry) {
        let promise = if let Some(token) = &entry.token {
            ext_fungible_token::ext(token.clone())
                .with_attached_deposit(NearToken::from_yoctonear(1))
                .with_static_gas(GAS_FOR_FT_TRANSFER)
                .ft_transfer(entry.proposer.clone(), entry.amount, None)
        } else {
            Promise::new(entry.proposer.clone()).transfer(NearToken::from_yoctonear(entry.amount.0))
        };

        promise.then(Self::ext(env::current_account_id()).on_retry_refund_complete(id, entry));
    }

    /// Callback executed after *each* retry attempt.
    ///
    /// Removes the entry from `refund_list` only upon success so that callers
    /// may attempt again later if needed.
    #[private]
    pub fn on_retry_refund_complete(
        &mut self,
        id: RefundId,
        entry: RefundEntry,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) {
        self.log_gas_checkpoint("on_retry_refund_complete");

        if result.is_ok() {
            log_event!(
                "retry_refund_succeeded",
                near_sdk::serde_json::json!({
                    "id": id,
                    "vault": env::current_account_id(),
                })
            );
            return;
        }

        log_event!(
            "retry_refund_failed",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "refund_id": id,
            })
        );

        // Only add it back to the list if it has not expired
        let current_epoch = env::epoch_height();
        if current_epoch < entry.added_at_epoch + REFUND_EXPIRY_EPOCHS {
            self.add_refund_entry(entry.token, entry.proposer, entry.amount, Some(id));
        }
    }
}
