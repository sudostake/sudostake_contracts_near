#![allow(dead_code)]

use crate::contract::{Vault, VaultExt};
use crate::log_event;
use crate::types::{ProcessingState, GAS_FOR_CALLBACK};

use near_sdk::json_types::U128;
use near_sdk::{env, near_bindgen, AccountId, NearToken};
use near_sdk::{require, Promise};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn claim_vault(&mut self) -> Promise {
        self.assert_vault_listed_for_takeover();
        self.ensure_processing_idle();

        let claimant = env::predecessor_account_id();
        self.assert_not_current_owner(&claimant);

        let purchase_price = self.get_storage_cost();
        let attached_deposit = env::attached_deposit();
        self.assert_exact_purchase_price(purchase_price, &attached_deposit);

        let previous_owner = self.owner.clone();
        self.begin_takeover();

        Promise::new(previous_owner.clone())
            .transfer(attached_deposit)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_claim_vault_complete(previous_owner, claimant, purchase_price),
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

        let claim_lock_held = self.processing_state == ProcessingState::ClaimVault;

        if let Err(_err) = result {
            self.handle_transfer_failure(new_owner, amount);
            self.release_claim_lock_if_needed(claim_lock_held);
            return;
        }

        if self.owner != old_owner {
            self.handle_stale_takeover(old_owner, new_owner, amount);
            self.release_claim_lock_if_needed(claim_lock_held);
            return;
        }

        self.finalize_successful_takeover(old_owner, new_owner, amount);
        self.release_claim_lock_if_needed(claim_lock_held);
    }
}

impl Vault {
    fn assert_vault_listed_for_takeover(&self) {
        require!(
            self.is_listed_for_takeover,
            "Vault is not listed for takeover"
        );
    }

    fn assert_not_current_owner(&self, claimant: &AccountId) {
        require!(
            claimant != &self.owner,
            "Current vault owner cannot claim their own vault"
        );
    }

    fn assert_exact_purchase_price(&self, expected_price: u128, attached: &NearToken) {
        require!(
            attached.as_yoctonear() == expected_price,
            format!(
                "Must attach exactly {} yoctoNEAR to claim the vault",
                expected_price
            )
        );
    }

    fn begin_takeover(&mut self) {
        self.acquire_processing_lock(ProcessingState::ClaimVault);
        self.is_listed_for_takeover = false;
    }

    fn handle_transfer_failure(&mut self, new_owner: AccountId, amount: u128) {
        log_event!(
            "claim_vault_failed",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "new_owner": new_owner,
                "amount": amount.to_string()
            })
        );

        self.relist_and_queue_refund(new_owner, amount);
    }

    fn handle_stale_takeover(
        &mut self,
        expected_owner: AccountId,
        new_owner: AccountId,
        amount: u128,
    ) {
        log_event!(
            "claim_vault_stale",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "expected_owner": expected_owner,
                "observed_owner": self.owner,
                "new_owner": new_owner
            })
        );

        self.relist_and_queue_refund(new_owner, amount);
    }

    fn finalize_successful_takeover(
        &mut self,
        old_owner: AccountId,
        new_owner: AccountId,
        amount: u128,
    ) {
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
    }

    fn relist_and_queue_refund(&mut self, beneficiary: AccountId, amount: u128) {
        self.is_listed_for_takeover = true;
        let _ = self.add_refund_entry(None, beneficiary, U128(amount), None, None);
    }

    fn release_claim_lock_if_needed(&mut self, held: bool) {
        if held {
            self.release_processing_lock();
        }
    }
}
