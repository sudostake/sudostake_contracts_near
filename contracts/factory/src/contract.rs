use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::serde::Serialize;
use near_sdk::{env, near_bindgen, AccountId, Promise};
use near_sdk::{Gas, NearToken};

use crate::log_event;

// NEAR costs (yoctoNEAR)
const STORAGE_BUFFER: u128 = 10_000_000_000_000_000_000_000; // 0.01 NEAR
const GAS_FOR_VAULT_INIT: Gas = Gas::from_tgas(100);

// vault.wasm bytes
const VAULT_WASM_BYTES: &[u8] = include_bytes!("../../../res/vault.wasm");

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct FactoryViewState {
    pub owner: AccountId,
    pub vault_minting_fee: NearToken,
    pub vault_counter: u64,
}

#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize)]
pub struct FactoryContract {
    pub owner: AccountId,
    pub vault_counter: u64,
    pub vault_minting_fee: NearToken,
}

impl Default for FactoryContract {
    fn default() -> Self {
        panic!("FactoryContract must be initialized with `new()`")
    }
}

#[near_bindgen]
impl FactoryContract {
    #[allow(dead_code)]
    #[init]
    pub fn new(owner: AccountId, vault_minting_fee: NearToken) -> Self {
        assert!(!env::state_exists(), "Already initialized");

        Self {
            owner,
            vault_counter: 0,
            vault_minting_fee,
        }
    }

    #[allow(dead_code)]
    pub fn set_vault_creation_fee(&mut self, new_fee: NearToken) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the factory owner can update the vault creation fee"
        );

        self.vault_minting_fee = new_fee;

        log_event!(
            "vault_creation_fee_updated",
            near_sdk::serde_json::json!({
               "new_fee": new_fee.as_yoctonear().to_string()
            })
        );
    }

    #[payable]
    #[allow(dead_code)]
    pub fn mint_vault(&mut self) -> Promise {
        // Get the caller and the attached deposit
        let caller = env::predecessor_account_id();
        let deposit = env::attached_deposit();

        // Ensure the minting fee has been configured
        let required_fee = self.vault_minting_fee.as_yoctonear();
        assert!(required_fee > 0, "Vault creation fee has not been set");

        // Ensure the attached deposit matches the required fee exactly
        assert_eq!(
            deposit.as_yoctonear(),
            required_fee,
            "Must attach exactly the vault minting fee"
        );

        // Load the vault code from state
        let vault_code: Vec<u8> = VAULT_WASM_BYTES.to_vec();

        // Estimate deployment cost based on WASM size and protocol storage pricing
        let wasm_bytes = vault_code.len() as u128;
        let deploy_cost = wasm_bytes * env::storage_byte_cost().as_yoctonear();
        let transfer_amount = deploy_cost + STORAGE_BUFFER;

        // Ensure the attached fee is sufficient for storage transfer
        assert!(
            required_fee >= transfer_amount,
            "Vault minting fee is too low to cover deployment"
        );

        // Generate a vault subaccount name
        let index = self.vault_counter;
        let vault_account_id = format!("vault-{}.{}", index, env::current_account_id());
        let vault_account: AccountId = vault_account_id.parse().expect("Invalid account ID");

        // Increment counter to prevent collisions
        self.vault_counter += 1;

        log_event!(
            "vault_minted",
            near_sdk::serde_json::json!({
                "owner": caller,
                "vault": vault_account,
                "index": index,
                "version": 1,
            })
        );

        // Prepare init arguments for the vault contract
        let json_args = near_sdk::serde_json::to_vec(&near_sdk::serde_json::json!({
            "owner": caller,
            "index": index,
            "version": 1,
        }))
        .unwrap();

        // Create the subaccount, deploy code, and call the init method
        Promise::new(vault_account)
            .create_account()
            .transfer(NearToken::from_yoctonear(transfer_amount))
            .deploy_contract(vault_code)
            .function_call(
                String::from("new"),
                json_args,
                NearToken::from_yoctonear(0),
                GAS_FOR_VAULT_INIT,
            )
    }

    #[allow(dead_code)]
    pub fn withdraw_balance(
        &mut self,
        amount: NearToken,
        to_address: Option<AccountId>,
    ) -> Promise {
        // Ensure only the factory owner can call this method
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the factory owner can withdraw balance"
        );

        // Use the provided recipient, or default to the factory owner
        let recipient = to_address.unwrap_or_else(|| self.owner.clone());

        // Get the total balance held by the factory contract (in yoctoNEAR)
        let total_balance: u128 = env::account_balance().as_yoctonear();

        // Compute required storage reserve (in yoctoNEAR)
        let storage_cost: u128 =
            env::storage_byte_cost().as_yoctonear() * env::storage_usage() as u128;

        // Subtract reserve to get available amount
        let available_balance = total_balance.saturating_sub(storage_cost);

        // Convert requested withdrawal amount to yoctoNEAR
        let amount_yocto = amount.as_yoctonear();

        // Ensure requested amount does not exceed safe withdrawal
        assert!(
            amount_yocto <= available_balance,
            "Requested amount exceeds available withdrawable balance"
        );

        // Transfer amount to the recipient
        Promise::new(recipient).transfer(amount)
    }

    #[allow(dead_code)]
    pub fn transfer_ownership(&mut self, new_owner: AccountId) {
        // Ensure only the current factory owner can transfer ownership
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the factory owner can transfer ownership"
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
                "factory_id": env::current_account_id(),
                "old_owner": old_owner,
                "new_owner": new_owner
            })
        );
    }
}

#[near_bindgen]
impl FactoryContract {
    #[allow(dead_code)]
    pub fn storage_byte_cost(&self) -> NearToken {
        env::storage_byte_cost()
    }

    #[allow(dead_code)]
    pub fn get_contract_state(&self) -> FactoryViewState {
        FactoryViewState {
            owner: self.owner.clone(),
            vault_minting_fee: self.vault_minting_fee,
            vault_counter: self.vault_counter,
        }
    }
}
