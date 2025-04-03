use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::NearToken;
use near_sdk::{env, near_bindgen, AccountId};

#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize)]
pub struct FactoryContract {
    pub owner: AccountId,
    pub vault_code_versions: UnorderedMap<u64, Vec<u8>>,
    pub latest_vault_version: u64,
    pub vault_counter: u64,
    pub vault_minting_fee: NearToken,
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
            vault_counter: 0,
            vault_minting_fee,
        }
    }
}
