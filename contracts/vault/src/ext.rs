use near_sdk::{ext_contract, json_types::U128, AccountId, NearToken, Promise};

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
}
