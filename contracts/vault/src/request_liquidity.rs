#![allow(dead_code)]

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::ext::ext_self;
use crate::log_event;
use crate::types::GAS_FOR_CALLBACK;
use crate::types::{LiquidityRequest, PendingLiquidityRequest};
use near_sdk::json_types::U128;
use near_sdk::require;
use near_sdk::PromiseResult;
use near_sdk::{assert_one_yocto, env, near_bindgen, AccountId, NearToken, Promise};

#[near_bindgen]
impl Vault {
    #[payable]
    pub fn request_liquidity(
        &mut self,
        token: AccountId,
        amount: U128,
        interest: U128,
        collateral: NearToken,
        duration: u64,
    ) -> Promise {
        // --- Permission & state checks ---
        assert_one_yocto();
        require!(
            env::predecessor_account_id() == self.owner,
            "Only the vault owner can request liquidity"
        );
        require!(
            self.pending_liquidity_request.is_none(),
            "A liquidity request is already in progress"
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

        // --- Temporarily store the request until stake is verified ---
        self.pending_liquidity_request = Some(PendingLiquidityRequest {
            token,
            amount,
            interest,
            collateral,
            duration,
        });

        // --- Batch query total staked balance across all active validators ---
        self.batch_query_total_staked(
            ext_self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_CALLBACK)
                .on_check_total_staked(),
        )
    }

    #[private]
    pub fn on_check_total_staked(&mut self) {
        self.log_gas_checkpoint("on_check_total_staked");

        // Retrieve and remove the pending request
        let pending = self
            .pending_liquidity_request
            .take()
            .expect("Expected a pending liquidity request");

        // Initialize total staked to zero
        let mut total_staked_yocto: u128 = 0;
        let num_results = env::promise_results_count();

        // Track and collect zero balance validators
        let validator_ids: Vec<AccountId> = self.active_validators.iter().collect();
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
                    } else {
                        env::log_str(&format!(
                            "Warning: Could not parse staked balance from result #{}",
                            i
                        ));
                    }
                }
                _ => {
                    env::log_str(&format!("Warning: Promise result #{} failed", i));
                }
            }
        }

        // Prune validators with zero staked balance
        for validator in zero_balance_validators {
            self.active_validators.remove(&validator);
            env::log_str(&format!(
                "Removed validator with zero staked balance: {}",
                validator
            ));
        }

        // Verify the total staked >= collateral
        let total_staked = NearToken::from_yoctonear(total_staked_yocto);
        require!(
            total_staked >= pending.collateral,
            "Insufficient staked NEAR to satisfy requested collateral"
        );

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
                "token": self.liquidity_request.as_ref().unwrap().token,
                "amount": pending.amount,
                "interest": pending.interest,
                "collateral": pending.collateral,
                "duration": pending.duration
            })
        );
    }
}
