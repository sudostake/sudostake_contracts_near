use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::NearToken;
use near_sdk::{env, near_bindgen, AccountId};
use sha2::{Digest, Sha256};

#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize)]
pub struct FactoryContract {
    pub owner: AccountId,
    pub vault_code_versions: UnorderedMap<Vec<u8>, Vec<u8>>, // key = hash
    pub latest_vault_version: u64,
    pub latest_vault_hash: Vec<u8>,
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
            vault_code_versions: UnorderedMap::new(b"v".to_vec()),
            latest_vault_version: 0,
            latest_vault_hash: vec![],
            vault_counter: 0,
            vault_minting_fee,
        }
    }

    #[allow(dead_code)]
    pub fn set_vault_code(&mut self, code: Vec<u8>) -> Vec<u8> {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only the factory owner can set new vault code"
        );

        // TODO apply comprehensive tests to make sure the uploaded vault wasm
        // is valid and matches expectations
        assert!(code.len() > 1024, "Vault code is too small to be valid");

        // Compute SHA-256 hash of the code
        let hash = Sha256::digest(&code).to_vec();

        // Prevent duplicate upload
        if self.vault_code_versions.get(&hash).is_some() {
            env::panic_str("This vault code has already been uploaded");
        }

        // Store by hash
        self.vault_code_versions.insert(&hash, &code);

        // Update versioning state
        self.latest_vault_version += 1;
        self.latest_vault_hash = hash.clone();

        // Emit log
        env::log_str(&format!(
            "Vault code v{} uploaded ({} bytes), hash: {}",
            self.latest_vault_version,
            code.len(),
            hex::encode(&hash)
        ));

        hash
    }
}
