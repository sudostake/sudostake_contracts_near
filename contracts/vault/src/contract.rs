use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, near_bindgen, AccountId, NearToken, Promise};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct Vault {
    owner: AccountId,
    index: u64,
    version: u64,
}

#[near_bindgen]
impl Vault {
    #[init]
    pub fn new(owner: AccountId, index: u64, version: u64) -> Self {
        assert!(!env::state_exists(), "Contract already initialized");

        let initial_storage = env::storage_usage();

        let instance = Self {
            owner,
            index,
            version,
        };

        let final_storage = env::storage_usage();
        let storage_cost = env::storage_byte_cost()
            .as_yoctonear()
            .saturating_mul((final_storage - initial_storage) as u128);
        let attached_deposit = env::attached_deposit().as_yoctonear();

        assert!(
            attached_deposit >= storage_cost,
            "Insufficient deposit for storage cost"
        );

        if attached_deposit > storage_cost {
            let refund = NearToken::from_yoctonear(attached_deposit - storage_cost);
            Promise::new(env::predecessor_account_id()).transfer(refund);
        }

        instance
    }
}
