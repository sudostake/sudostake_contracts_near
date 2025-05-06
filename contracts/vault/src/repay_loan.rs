#![allow(dead_code)]

use crate::{
    contract::{Vault, VaultExt},
    ext::ext_fungible_token,
    log_event,
    types::{ProcessingState, GAS_FOR_CALLBACK, GAS_FOR_FT_TRANSFER},
};
use near_sdk::{
    assert_one_yocto, env, json_types::U128, near_bindgen, require, NearToken, Promise,
    PromiseError,
};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn repay_loan(&mut self) -> Promise {
        // Require 1 yoctoNEAR for access control
        assert_one_yocto();

        // Only the vault owner can perform this action
        require!(
            env::predecessor_account_id() == self.owner,
            "Only the vault owner can repay the loan"
        );

        // Ensure there is a liquidity request
        let request = self
            .liquidity_request
            .as_ref()
            .expect("No active loan to repay");

        // Ensure the request has been accepted
        let offer = self
            .accepted_offer
            .as_ref()
            .expect("No accepted offer found");

        // Ensure liquidation has not started
        require!(
            self.liquidation.is_none(),
            "Loan has already entered liquidation"
        );

        // Calculate total amount due: principal + interest
        let total_due = U128(request.amount.0 + request.interest.0);
        let lender = offer.lender.clone();
        let token = request.token.clone();

        // Lock the vault for **RepayLoan** workflow
        self.acquire_processing_lock(ProcessingState::RepayLoan);

        // Transfer the total repayment to the lender
        ext_fungible_token::ext(token)
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .ft_transfer(lender, total_due, None)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_repay_loan(),
            )
    }

    #[private]
    pub fn on_repay_loan(&mut self, #[callback_result] result: Result<(), PromiseError>) {
        // Inspect amount of gas left
        self.log_gas_checkpoint("on_repay_loan");
        self.release_processing_lock();

        // Log repay_loan_failed event
        if result.is_err() {
            log_event!(
                "repay_loan_failed",
                near_sdk::serde_json::json!({
                    "vault": env::current_account_id(),
                    "error": "ft_transfer to lender failed"
                })
            );

            return;
        }

        // Loan was successfully repaid — clear loan state
        self.accepted_offer = None;
        self.liquidity_request = None;

        // Log repay_loan_successful event
        log_event!(
            "repay_loan_successful",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
            })
        );
    }
}
