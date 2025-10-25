#![allow(dead_code)]

use crate::contract::{Vault, VaultExt};
use crate::log_event;
use crate::types::{ProcessingState, GAS_FOR_CALLBACK};

use near_sdk::json_types::U128;
use near_sdk::{env, near_bindgen, AccountId};
use near_sdk::{require, Promise};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn claim_vault(&mut self) -> Promise {
        require!(
            self.is_listed_for_takeover,
            "Vault is not listed for takeover"
        );

        self.ensure_processing_idle();

        let caller = env::predecessor_account_id();
        require!(
            caller != self.owner,
            "Current vault owner cannot claim their own vault"
        );

        let price = self.get_storage_cost();
        let deposit = env::attached_deposit();
        require!(
            deposit.as_yoctonear() == price,
            format!("Must attach exactly {} yoctoNEAR to claim the vault", price)
        );

        let old_owner = self.owner.clone();

        // Prevent concurrent claims while the transfer is in flight
        self.acquire_processing_lock(ProcessingState::ClaimVault);
        self.is_listed_for_takeover = false;

        // Proceed with transfer, and finalize in callback
        Promise::new(old_owner.clone()).transfer(deposit).then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_CALLBACK)
                .on_claim_vault_complete(old_owner, caller, price),
        )
    }

    #[private]
    pub fn on_claim_vault_complete(
        &mut self,
        old_owner: AccountId,
        new_owner: AccountId,
        amount: u128,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) {
        self.log_gas_checkpoint("on_claim_vault_complete");

        let release_claim_lock = self.processing_state == ProcessingState::ClaimVault;

        if result.is_err() {
            log_event!(
                "claim_vault_failed",
                near_sdk::serde_json::json!({
                   "vault": env::current_account_id(),
                   "new_owner": new_owner,
                   "amount": amount.to_string()
                })
            );

            // Restore listing so another claimant can try again
            self.is_listed_for_takeover = true;
            let _ = self.add_refund_entry(None, new_owner, U128(amount), None, None);
            if release_claim_lock {
                self.release_processing_lock();
            }
            return;
        }

        // Abort if vault ownership changed while the transfer was in flight
        if self.owner != old_owner {
            log_event!(
                "claim_vault_stale",
                near_sdk::serde_json::json!({
                    "vault": env::current_account_id(),
                    "expected_owner": old_owner,
                    "observed_owner": self.owner,
                    "new_owner": new_owner
                })
            );

            self.is_listed_for_takeover = true;
            let _ = self.add_refund_entry(None, new_owner, U128(amount), None, None);
            if release_claim_lock {
                self.release_processing_lock();
            }
            return;
        }

        self.owner = new_owner.clone();
        self.is_listed_for_takeover = false;

        log_event!(
            "vault_claimed",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "old_owner": old_owner,
                "new_owner": new_owner,
                "amount": amount.to_string()
            })
        );

        if release_claim_lock {
            self.release_processing_lock();
        }
    }
}
