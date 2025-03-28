use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::{env, near_bindgen, AccountId, BorshStorageKey, Gas, NearToken, Promise};

const VAULT_CREATION_FEE: NearToken = NearToken::from_yoctonear(1_000_000_000_000_000_000_000_000); // 1 NEAR in yoctoNEAR

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct Factory {
    /// Stores available vault contract versions
    vault_code_versions: UnorderedMap<u64, Vec<u8>>, // Store actual Wasm code
    /// Tracks the latest vault version
    latest_vault_version: u64,
    /// Tracks the next vault index
    vault_counter: u64,
    /// Owner of the factory contract
    owner: AccountId,
}

#[derive(BorshStorageKey, BorshSerialize)]
pub enum StorageKey {
    VaultCodeVersions,
}

impl Default for Factory {
    fn default() -> Self {
        env::panic_str("Factory must be initialized with an owner");
    }
}

#[near_bindgen]
impl Factory {
    #[init]
    pub fn new(owner: AccountId) -> Self {
        Self {
            vault_code_versions: UnorderedMap::new(StorageKey::VaultCodeVersions),
            latest_vault_version: 0,
            vault_counter: 0,
            owner,
        }
    }

    /// Add a new vault contract version
    pub fn add_vault_version(&mut self, contract_wasm: Vec<u8>) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can add vault versions"
        );
        let version = self.latest_vault_version + 1;

        self.vault_code_versions.insert(&version, &contract_wasm);
        self.latest_vault_version = version;
    }

    /// Get a list of available vault versions
    pub fn get_vault_versions(&self) -> Vec<u64> {
        self.vault_code_versions.keys_as_vector().to_vec()
    }

    /// Create a new vault instance as a sub-account of the factory contract
    #[payable]
    pub fn create_vault(&mut self) -> Promise {
        assert!(self.latest_vault_version > 0, "No vault versions available");

        let attached_deposit = env::attached_deposit();
        assert_eq!(
            attached_deposit.as_yoctonear(),
            VAULT_CREATION_FEE.as_yoctonear(),
            "Deposit must be exactly {} yoctoNEAR",
            VAULT_CREATION_FEE.as_yoctonear()
        );

        let selected_version = self.latest_vault_version;
        let contract_code = self
            .vault_code_versions
            .get(&selected_version)
            .expect("Vault code not found");

        self.vault_counter = self
            .vault_counter
            .checked_add(1)
            .expect("Vault counter overflow");

        let factory_account = env::current_account_id();
        let vault_subaccount = format!("vault_{}.{}", self.vault_counter, factory_account);
        let vault_account: AccountId = vault_subaccount
            .parse()
            .expect("Failed to create valid vault account");
        let owner_id = env::predecessor_account_id();

        Promise::new(vault_account.clone())
            .create_account()
            .transfer(VAULT_CREATION_FEE)
            .deploy_contract(contract_code)
            .function_call(
                "init".to_string(),
                json!({
                    "owner_id": owner_id,
                    "index": self.vault_counter, // Use assigned vault index
                    "version": selected_version
                })
                .to_string()
                .into_bytes(),
                NearToken::from_yoctonear(0),
                Gas::from_gas(env::prepaid_gas().as_gas() / 2), // Ensure enough gas for execution
            )
    }

    /// Withdraw vault creation fees based on available balance
    pub fn withdraw_fees(&mut self, to: AccountId, amount: U128) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only contract owner can withdraw fees"
        );
        assert!(amount.0 > 0, "Withdrawal amount must be greater than zero");

        let available_balance = env::account_balance().as_yoctonear();
        assert!(
            amount.0 <= available_balance.saturating_sub(VAULT_CREATION_FEE.as_yoctonear()),
            "Cannot withdraw reserved vault fees"
        );

        Promise::new(to).transfer(NearToken::from_yoctonear(amount.0));
    }

    /// Transfer ownership of the factory contract
    pub fn transfer_ownership(&mut self, new_owner: AccountId) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the owner can transfer ownership"
        );
        self.owner = new_owner;
    }
}
