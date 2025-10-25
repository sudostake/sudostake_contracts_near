#![cfg_attr(not(test), allow(dead_code))]

use crate::contract::{Vault, VaultExt};
use crate::log_event;

use near_sdk::{assert_one_yocto, require};
use near_sdk::{env, near_bindgen};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn list_for_takeover(&mut self) {
        assert_one_yocto();

        require!(
            env::predecessor_account_id() == self.owner,
            "Only the vault owner can list the vault for takeover"
        );

        self.ensure_processing_idle();

        require!(
            !self.is_listed_for_takeover,
            "Vault is already listed for takeover"
        );

        self.is_listed_for_takeover = true;

        log_event!(
            "vault_listed_for_takeover",
            near_sdk::serde_json::json!({
                "owner": self.owner,
                "vault": env::current_account_id(),
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

        self.ensure_processing_idle();

        self.is_listed_for_takeover = false;

        log_event!(
            "vault_takeover_cancelled",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "owner": self.owner
            })
        );
    }
}
