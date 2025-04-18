#![allow(dead_code)]
use near_sdk::{ext_contract, json_types::U128, AccountId, NearToken, Promise};

#[ext_contract(ext_self)]
pub trait VaultExt {
    fn on_deposit_and_stake(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    );

    fn on_unstake(
        &mut self,
        validator: AccountId,
        amount: NearToken,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    );

    fn on_withdraw_all(
        &mut self,
        validator: AccountId,
        #[callback_result] result: Result<(), near_sdk::PromiseError>,
    ) -> Promise;

    fn on_check_total_staked(&mut self);

    fn on_refund_complete(&mut self, proposer: AccountId, amount: U128, token_address: AccountId);

    fn on_retry_refund_complete(&mut self, id: u64);

    fn on_repay_loan(&mut self, #[callback_result] result: Result<(), near_sdk::PromiseError>);
}

#[ext_contract(ext_staking_pool)]
pub trait StakingPool {
    fn deposit_and_stake(&self);
    fn unstake(&self, amount: U128);
    fn withdraw_all(&self);
    fn get_account_staked_balance(&self, account_id: AccountId) -> U128;
}

#[ext_contract(ext_fungible_token)]
pub trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}
