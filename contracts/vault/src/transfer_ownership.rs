#![allow(dead_code)]

use crate::contract::{Vault, VaultExt};
use crate::log_event;

use near_sdk::assert_one_yocto;
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

        self.ensure_processing_idle();

        // Prevent transferring to the same owner
        assert_ne!(
            new_owner, self.owner,
            "New owner must be different from the current owner"
        );

        // Update owner state
        let old_owner = self.owner.clone();
        self.owner = new_owner.clone();
        self.is_listed_for_takeover = false;

        // Emit log
        log_event!(
            "ownership_transferred",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "old_owner": old_owner,
                "new_owner": new_owner
            })
        );
    }
}
