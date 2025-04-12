use near_sdk::{ext_contract, json_types::U128, AccountId, NearToken, Promise};

// staking_pool ext call method names
pub const METHOD_DEPOSIT_AND_STAKE: &str = "deposit_and_stake";
pub const METHOD_UNSTAKE: &str = "unstake";
pub const METHOD_WITHDRAW_ALL: &str = "withdraw_all";
pub const METHOD_GET_ACCOUNT_STAKED_BALANCE: &str = "get_account_staked_balance";
pub const METHOD_GET_ACCOUNT_UNSTAKED_BALANCE: &str = "get_account_unstaked_balance";

#[ext_contract(ext_self)]
pub trait VaultExt {
    fn on_withdraw_all_returned_for_delegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
    ) -> Promise;

    fn on_account_unstaked_balance_returned_for_delegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    ) -> Promise;

    fn on_deposit_and_stake_returned_for_delegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    );

    fn on_account_staked_balance_returned_for_undelegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    ) -> Promise;

    fn on_withdraw_all_returned_for_undelegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        should_remove_validator: bool,
    ) -> Promise;

    fn on_account_unstaked_balance_returned_for_undelegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        should_remove_validator: bool,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    ) -> Promise;

    fn on_unstake_returned_for_undelegate(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        should_remove_validator: bool,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    );

    fn on_withdraw_all_returned_for_claim_unstaked(&mut self, validator: AccountId) -> Promise;

    fn on_account_unstaked_balance_returned_for_claim_unstaked(
        &mut self,
        validator: AccountId,
        #[callback_result] result: Result<U128, near_sdk::PromiseError>,
    );

    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);

    fn get_account_staked_balance(&self, account_id: AccountId) -> U128;

    fn on_check_total_staked(&mut self);
}
