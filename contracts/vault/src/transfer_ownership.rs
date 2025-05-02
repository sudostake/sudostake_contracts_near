#![allow(dead_code)]

use crate::contract::{Vault, VaultExt};
use crate::log_event;
use crate::types::{RefundEntry, GAS_FOR_CALLBACK};

use near_sdk::json_types::U128;
use near_sdk::{assert_one_yocto, require, Promise};
use near_sdk::{env, near_bindgen, AccountId};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn transfer_ownership(&mut self, new_owner: AccountId) {
        // Require 1 yoctoNEAR to prevent accidental calls
        assert_one_yocto();

        // Ensure only the current vault owner can transfer ownership
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the vault owner can transfer ownership"
        );

        // Prevent transferring to the same owner
        assert_ne!(
            new_owner, self.owner,
            "New owner must be different from the current owner"
        );

        // Update owner state
        let old_owner = self.owner.clone();
        self.owner = new_owner.clone();

        // Emit log
        log_event!(
            "ownership_transferred",
            near_sdk::serde_json::json!({
                "old_owner": old_owner,
                "new_owner": new_owner
            })
        );
    }

    #[payable]
    pub fn list_for_takeover(&mut self) {
        assert_one_yocto();

        require!(
            env::predecessor_account_id() == self.owner,
            "Only the vault owner can list the vault for takeover"
        );

        require!(
            !self.is_listed_for_takeover,
            "Vault is already listed for takeover"
        );

        self.is_listed_for_takeover = true;

        log_event!(
            "vault_listed_for_takeover",
            near_sdk::serde_json::json!({
                "owner": self.owner,
                "storage_cost": self.get_storage_cost().to_string()
            })
        );
    }

    #[payable]
    pub fn cancel_takeover(&mut self) {
        assert_one_yocto();
        require!(
            env::predecessor_account_id() == self.owner,
            "Only the vault owner can cancel takeover"
        );
        require!(
            self.is_listed_for_takeover,
            "Vault is not listed for takeover"
        );

        self.is_listed_for_takeover = false;

        log_event!(
            "vault_takeover_cancelled",
            near_sdk::serde_json::json!({
                "owner": self.owner
            })
        );
    }

    #[payable]
    pub fn claim_vault(&mut self) -> Promise {
        require!(
            self.is_listed_for_takeover,
            "Vault is not listed for takeover"
        );

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

        if result.is_err() {
            let id = self.get_refund_nonce();
            self.refund_list.insert(
                &id,
                &RefundEntry {
                    token: None,
                    proposer: new_owner,
                    amount: U128(amount),
                },
            );

            env::panic_str("Vault takeover failed. You may call retry_refunds later.");
        }

        self.owner = new_owner.clone();
        self.is_listed_for_takeover = false;

        log_event!(
            "vault_claimed",
            near_sdk::serde_json::json!({
                "old_owner": old_owner,
                "new_owner": new_owner,
                "amount": amount.to_string()
            })
        );
    }
}
