#![allow(dead_code)]

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::log_event;
use crate::types::ProcessingState;
use crate::types::GAS_FOR_CALLBACK;
use crate::types::{LiquidityRequest, PendingLiquidityRequest, MAX_LOAN_DURATION};
use near_sdk::json_types::U128;
use near_sdk::require;
use near_sdk::PromiseResult;
use near_sdk::{assert_one_yocto, env, near_bindgen, AccountId, NearToken, Promise};

#[near_bindgen]
impl Vault {
    /// Owner requests a loan backed by staked NEAR.
    ///
    /// This flow is **mutually exclusive** with every other async workflow
    /// (undelegate, process_claims, repay, …) thanks to the global lock.
    #[payable]
    pub fn request_liquidity(
        &mut self,
        token: AccountId,
        amount: U128,
        interest: U128,
        collateral: NearToken,
        duration: u64,
    ) -> Promise {
        assert_one_yocto();
        require!(
            env::predecessor_account_id() == self.owner,
            "Only the vault owner can request liquidity"
        );
        require!(
            self.liquidity_request.is_none(),
            "A request is already open"
        );
        require!(
            self.accepted_offer.is_none(),
            "Vault is already matched with a lender"
        );
        require!(
            self.counter_offers.is_none(),
            "Counter-offers must be cleared"
        );
        require!(
            collateral > NearToken::from_yoctonear(0),
            "Collateral must be positive"
        );
        require!(amount.0 > 0, "Requested amount must be greater than zero");
        require!(duration > 0, "Duration must be non-zero");
        require!(
            duration <= MAX_LOAN_DURATION,
            format!(
                "Loan duration exceeds maximum allowed value ({} seconds)",
                MAX_LOAN_DURATION
            )
        );

        // Lock the vault for **RequestLiquidity** workflow
        self.acquire_processing_lock(ProcessingState::RequestLiquidity);

        // Prepare the request details to be verified once staking balances return
        let request = PendingLiquidityRequest {
            token,
            amount,
            interest,
            collateral,
            duration,
        };

        // Batch query total staked balance across all active validators
        let validators = self.get_ordered_validator_list();
        let cb = Self::ext(env::current_account_id())
            .with_static_gas(GAS_FOR_CALLBACK)
            .on_check_total_staked(validators.clone(), request);
        self.batch_query_total_staked(&validators, cb)
    }

    #[private]
    pub fn on_check_total_staked(
        &mut self,
        validator_ids: Vec<AccountId>,
        pending: PendingLiquidityRequest,
    ) {
        self.log_gas_checkpoint("on_check_total_staked");

        // Initialize total staked to zero
        let mut total_staked_yocto: u128 = 0;
        let num_results = env::promise_results_count();

        // Track and collect zero balance validators
        let mut zero_balance_validators: Vec<AccountId> = vec![];

        // Iterate through the results and calculate total_staked_yocto
        for i in 0..num_results {
            let validator_id = &validator_ids[i as usize];
            match env::promise_result(i) {
                PromiseResult::Successful(bytes) => {
                    if let Ok(U128(staked)) = near_sdk::serde_json::from_slice::<U128>(&bytes) {
                        total_staked_yocto += staked;
                        if staked == 0 {
                            zero_balance_validators.push(validator_id.clone());
                        }
                    }
                }
                _ => env::log_str(&format!("Warning: promise result #{} failed", i)),
            }
        }

        // prune zero‑stake validators
        for v in zero_balance_validators {
            self.active_validators.remove(&v);
        }

        // Verify the total staked >= collateral
        let total_staked = NearToken::from_yoctonear(total_staked_yocto);
        if total_staked < pending.collateral {
            log_event!(
                "liquidity_request_failed_insufficient_stake",
                near_sdk::serde_json::json!({
                    "vault": env::current_account_id(),
                    "required_collateral": pending.collateral,
                    "total_staked": total_staked
                })
            );

            // release lock & exit
            self.release_processing_lock();
            return;
        }

        // Finalize liquidity request
        self.liquidity_request = Some(LiquidityRequest {
            token: pending.token,
            amount: pending.amount,
            interest: pending.interest,
            collateral: pending.collateral,
            duration: pending.duration,
            created_at: env::block_timestamp(),
        });

        // Log liquidity_request_opened event
        log_event!(
            "liquidity_request_opened",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "token": self.liquidity_request.as_ref().unwrap().token,
                "amount": pending.amount,
                "interest": pending.interest,
                "collateral": pending.collateral,
                "duration": pending.duration
            })
        );

        // Finally release the lock
        self.release_processing_lock();
    }
}
