#![allow(dead_code)]

use crate::contract::Vault;
use crate::contract::VaultExt;
use crate::ext::{ext_self, ext_staking_pool};
use crate::log_event;
use crate::types::{
    LiquidityRequest, PendingLiquidityRequest, StorageKey, GAS_FOR_CALLBACK, GAS_FOR_VIEW_CALL,
};
use near_sdk::collections::UnorderedMap;
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

        // --- Ensure there are validators to query ---
        let mut validators = self.active_validators.iter();
        let first = validators
            .next()
            .expect("No active validators available for collateral check");

        // --- Start staking view call chain ---
        let initial = ext_staking_pool::ext(first.clone())
            .with_static_gas(GAS_FOR_VIEW_CALL)
            .get_account_staked_balance(env::current_account_id());

        let promise_chain = validators.fold(initial, |acc, validator| {
            acc.and(
                ext_staking_pool::ext(validator.clone())
                    .with_static_gas(GAS_FOR_VIEW_CALL)
                    .get_account_staked_balance(env::current_account_id()),
            )
        });

        // Attach the final callback to check total staked balance
        promise_chain.then(
            ext_self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_CALLBACK)
                .on_check_total_staked(),
        )
    }

    #[private]
    pub fn on_check_total_staked(&mut self) {
        // Retrieve and remove the pending request
        let pending = self
            .pending_liquidity_request
            .take()
            .expect("Expected a pending liquidity request");

        // Initialize total staked to zero
        let mut total_staked_yocto: u128 = 0;
        let num_results = env::promise_results_count();

        // Iterate through the results and calculate total_staked_yocto
        for i in 0..num_results {
            match env::promise_result(i) {
                PromiseResult::Successful(bytes) => {
                    if let Ok(U128(staked)) = near_sdk::serde_json::from_slice::<U128>(&bytes) {
                        total_staked_yocto += staked;
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

        // Initialize counter offers map
        self.counter_offers = Some(UnorderedMap::new(StorageKey::CounterOffers));

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
