#[cfg(test)]
mod tests {
    use crate::contract::FactoryContract;
    use near_sdk::{test_utils::VMContextBuilder, testing_env, AccountId, NearToken};

    #[test]
    fn test_factory_initialization() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let minting_fee = NearToken::from_yoctonear(1_000_000_000_000_000_000_000_000);

        let context = VMContextBuilder::new()
            .signer_account_id(owner.clone())
            .build();
        testing_env!(context);

        let contract = FactoryContract::new(owner.clone(), minting_fee);

        assert_eq!(contract.owner, owner);
        assert_eq!(contract.latest_vault_version, 0);
        assert_eq!(contract.vault_counter, 0);
        assert_eq!(contract.vault_minting_fee, minting_fee);
    }
}
