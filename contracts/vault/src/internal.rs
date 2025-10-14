use crate::contract::Vault;
use crate::ext::{ext_fungible_token, ext_self, ext_staking_pool};
use crate::log_event;
use crate::types::{
    CounterOffer, ProcessingState, RefundEntry, UnstakeEntry, GAS_FOR_FT_TRANSFER,
    GAS_FOR_VIEW_CALL, GAS_FOR_WITHDRAW_ALL, LOCK_TIMEOUT, STORAGE_BUFFER,
};
use near_sdk::json_types::U128;
use near_sdk::{env, require, AccountId, EpochHeight, NearToken, Promise};

/// Internal utility methods for the Vault contract.
impl Vault {
    /// Calculates the total storage cost for the contract, including a reserved buffer.
    /// This is used to prevent accidental contract deletion when reducing balance.
    pub(crate) fn get_storage_cost(&self) -> u128 {
        let actual_cost = env::storage_byte_cost().as_yoctonear() * env::storage_usage() as u128;
        actual_cost + STORAGE_BUFFER
    }

    /// Returns the NEAR balance available for operations, after reserving storage buffer.
    pub(crate) fn get_available_balance(&self) -> NearToken {
        let total = env::account_balance().as_yoctonear();
        let available = total.saturating_sub(self.get_storage_cost());
        NearToken::from_yoctonear(available)
    }

    /// Increments and returns the current refund nonce.
    /// Used to generate unique keys for refund entries.
    pub(crate) fn get_refund_nonce(&mut self) -> u64 {
        let id = self.refund_nonce;
        self.refund_nonce += 1;
        id
    }

    /// Logs a checkpoint showing the remaining prepaid gas for debugging purposes.
    /// The `method` argument tags the checkpoint for traceability.
    pub(crate) fn log_gas_checkpoint(&self, method: &str) {
        let gas_left = env::prepaid_gas().as_gas() - env::used_gas().as_gas();
        log_event!(
            "gas_check",
            near_sdk::serde_json::json!({
                "method": method,
                "gas_left": gas_left
            })
        );
    }

    /// Refunds all active counter offers by initiating `ft_transfer` calls
    /// for each proposer using the provided token contract.
    pub(crate) fn refund_all_counter_offers(&mut self, token: AccountId) {
        if let Some(mut counter_offers) = self.counter_offers.take() {
            let pending_refunds: Vec<CounterOffer> = counter_offers.values().collect();

            // Explicitly clear storage so stale offers do not linger between requests.
            counter_offers.clear();

            self.counter_offers = None;

            self.schedule_counter_offer_refunds(token, pending_refunds);
        }
    }

    pub(crate) fn schedule_counter_offer_refunds(
        &mut self,
        token: AccountId,
        offers: Vec<CounterOffer>,
    ) {
        if offers.is_empty() {
            return;
        }

        if offers.len() == 1 {
            let offer = offers.into_iter().next().expect("len checked above");
            let _ = self.refund_counter_offer(token, offer);
            return;
        }

        let mut iter = offers.into_iter();
        let first_offer = iter.next().expect("len checked above");
        let mut refund_metadata: Vec<(u64, AccountId, U128)> = Vec::with_capacity(1 + iter.len());

        let first_refund_id = self.add_refund_entry(
            Some(token.clone()),
            first_offer.proposer.clone(),
            first_offer.amount,
            None,
            None,
        );

        refund_metadata.push((
            first_refund_id,
            first_offer.proposer.clone(),
            first_offer.amount,
        ));

        let mut batch = ext_fungible_token::ext(token.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .ft_transfer(first_offer.proposer.clone(), first_offer.amount, None);

        for offer in iter {
            let refund_id = self.add_refund_entry(
                Some(token.clone()),
                offer.proposer.clone(),
                offer.amount,
                None,
                None,
            );

            refund_metadata.push((refund_id, offer.proposer.clone(), offer.amount));

            batch = batch.and(
                ext_fungible_token::ext(token.clone())
                    .with_attached_deposit(NearToken::from_yoctonear(1))
                    .with_static_gas(GAS_FOR_FT_TRANSFER)
                    .ft_transfer(offer.proposer.clone(), offer.amount, None),
            );
        }

        let _ = batch.then(
            ext_self::ext(env::current_account_id())
                .on_batch_refunds_complete(token, refund_metadata),
        );
    }

    /// Refunds a single counter offer by calling `ft_transfer`.
    /// A callback is attached to handle the refund result.
    pub(crate) fn refund_counter_offer(
        &mut self,
        token_address: AccountId,
        offer: CounterOffer,
    ) -> Promise {
        let refund_id = self.add_refund_entry(
            Some(token_address.clone()),
            offer.proposer.clone(),
            offer.amount,
            None,
            None,
        );

        ext_fungible_token::ext(token_address.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .ft_transfer(offer.proposer.clone(), offer.amount, None)
            .then(ext_self::ext(env::current_account_id()).on_refund_complete(
                refund_id,
                offer.proposer.clone(),
                offer.amount,
                token_address,
            ))
    }

    /// Chains `withdraw_all` calls for the given list of validators,
    /// and returns a promise that resolves to the provided callback.
    pub(crate) fn batch_claim_unstaked(
        &self,
        validators: Vec<AccountId>,
        call_back: Promise,
    ) -> Promise {
        let mut chain = ext_staking_pool::ext(validators[0].clone())
            .with_static_gas(GAS_FOR_WITHDRAW_ALL)
            .withdraw_all();

        for validator in validators.iter().skip(1) {
            chain = chain.and(
                ext_staking_pool::ext(validator.clone())
                    .with_static_gas(GAS_FOR_WITHDRAW_ALL)
                    .withdraw_all(),
            );
        }

        chain.then(call_back)
    }

    /// Returns the list of active validators in a stable, deterministic order.
    /// Currently we just sort lexicographically by the account ID;  
    /// this costs a few µgas but guarantees repeatable indexing.
    pub(crate) fn get_ordered_validator_list(&self) -> Vec<AccountId> {
        let mut list: Vec<AccountId> = self.active_validators.iter().collect();
        list.sort(); // `AccountId` == `String`, so `Ord` is available
        list
    }

    /// Queries `get_account_staked_balance` for every validator in `validator_ids`,
    /// chaining the results and finally executing `callback`.
    ///
    /// * Panics* if `validator_ids` is empty.
    pub(crate) fn batch_query_total_staked(
        &self,
        validator_ids: &[AccountId],
        callback: Promise,
    ) -> Promise {
        // Pre‑checks
        let mut iter = validator_ids.iter();
        let first = iter
            .next()
            .expect("`validator_ids` must contain at least one validator");

        // Build the promise chain
        let initial = ext_staking_pool::ext(first.clone())
            .with_static_gas(GAS_FOR_VIEW_CALL)
            .get_account_staked_balance(env::current_account_id());

        let chain = iter.fold(initial, |acc, validator| {
            acc.and(
                ext_staking_pool::ext(validator.clone())
                    .with_static_gas(GAS_FOR_VIEW_CALL)
                    .get_account_staked_balance(env::current_account_id()),
            )
        });

        // Attach final callback
        chain.then(callback)
    }

    /// Calls `unstake` for each (validator, amount) pair,
    /// then chains the results to a single callback promise.
    pub(crate) fn batch_unstake(
        &self,
        unstake_instructions: Vec<(AccountId, u128, bool)>,
        call_back: Promise,
    ) -> Promise {
        let mut chain = ext_staking_pool::ext(unstake_instructions[0].0.clone())
            .with_static_gas(crate::types::GAS_FOR_UNSTAKE)
            .unstake(U128::from(unstake_instructions[0].1));

        for (validator, amount, _) in unstake_instructions.iter().skip(1) {
            chain = chain.and(
                ext_staking_pool::ext(validator.clone())
                    .with_static_gas(crate::types::GAS_FOR_UNSTAKE)
                    .unstake(U128::from(*amount)),
            );
        }

        chain.then(call_back)
    }

    /// Records a failed refund operation into `refund_list`.
    /// Accepts an optional `refund_id`, otherwise assigns a new nonce.
    pub(crate) fn add_refund_entry(
        &mut self,
        token: Option<AccountId>,
        proposer: AccountId,
        amount: U128,
        refund_id: Option<u64>,
        added_at_epoch: Option<EpochHeight>,
    ) -> u64 {
        let id = refund_id.unwrap_or_else(|| self.get_refund_nonce());
        let epoch_recorded = added_at_epoch.unwrap_or_else(env::epoch_height);
        self.refund_list.insert(
            &id,
            &RefundEntry {
                token,
                proposer,
                amount,
                added_at_epoch: epoch_recorded,
            },
        );
        id
    }

    /// Updates (or creates) an unstake entry for a given validator by adding the provided amount.
    /// Overwrites the epoch with the current one.
    pub(crate) fn update_validator_unstake_entry(&mut self, validator: &AccountId, amount: u128) {
        let mut entry = self
            .unstake_entries
            .get(&validator)
            .unwrap_or_else(|| UnstakeEntry {
                amount: 0,
                epoch_height: 0,
            });

        entry.amount += amount;
        entry.epoch_height = env::epoch_height();
        self.unstake_entries.insert(&validator, &entry);
    }

    /// Attempts to acquire the global processing lock for a long-running operation (e.g., repay or claim).
    ///
    /// - Automatically releases stale locks if `LOCK_TIMEOUT` has passed.
    /// - Aborts if another operation is currently in progress.
    /// - Logs a `lock_acquired` event.
    pub(crate) fn acquire_processing_lock(&mut self, kind: ProcessingState) {
        assert!(kind != ProcessingState::Idle, "Cannot lock with Idle");

        let now = env::block_timestamp();
        // Saturating subtract guards against potential timestamp rollback, which would
        // otherwise underflow and panic when block timestamps decrease.
        let elapsed = now.saturating_sub(self.processing_since);

        if self.processing_state != ProcessingState::Idle && elapsed >= LOCK_TIMEOUT {
            self.processing_state = ProcessingState::Idle;
            self.processing_since = 0;
        }

        require!(
            self.processing_state == ProcessingState::Idle,
            format!("Vault busy with {:?}", self.processing_state)
        );

        self.processing_state = kind;
        self.processing_since = now;

        log_event!(
            "lock_acquired",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "kind": format!("{:?}", kind),
                "timestamp": now
            })
        );
    }

    /// Releases the currently held processing lock and resets the state to Idle.
    /// Logs a `lock_released` event with a timestamp.
    pub(crate) fn release_processing_lock(&mut self) {
        let now = env::block_timestamp();

        log_event!(
            "lock_released",
            near_sdk::serde_json::json!({
                "vault": env::current_account_id(),
                "kind": format!("{:?}", self.processing_state),
                "timestamp": now
            })
        );

        self.processing_state = ProcessingState::Idle;
        self.processing_since = 0;
    }
}
