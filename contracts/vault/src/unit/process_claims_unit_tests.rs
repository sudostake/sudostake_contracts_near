#![allow(clippy::too_many_lines)]

#[path = "test_utils.rs"]
mod test_utils;

use crate::{
    contract::Vault,
    types::{
        AcceptedOffer, Liquidation, ProcessingState, UnstakeEntry, LOCK_TIMEOUT, NANOS_PER_SECOND,
        NUM_EPOCHS_TO_UNLOCK,
    },
};
use near_sdk::{
    env,
    json_types::U128,
    mock::{MockAction, Receipt},
    serde_json,
    test_utils::{get_created_receipts, VMContextBuilder},
    test_vm_config, testing_env, AccountId, NearToken, PromiseResult, RuntimeFeesConfig,
};
use test_utils::{alice, create_valid_liquidity_request, owner, YOCTO_NEAR};

const VALIDATOR_A: &str = "validator-a.testnet";
fn expiry_timestamp() -> u64 {
    let request = create_valid_liquidity_request("usdc.test.near".parse().unwrap());
    request.duration * NANOS_PER_SECOND + 1
}

fn context_builder(balance_near: u128, attached_deposit: Option<u128>, now: u64, epoch: u64) -> VMContextBuilder {
    let mut builder = VMContextBuilder::new();
    builder
        .predecessor_account_id(owner())
        .account_balance(NearToken::from_near(balance_near))
        .epoch_height(epoch)
        .block_timestamp(now);

    if let Some(yocto) = attached_deposit {
        builder.attached_deposit(NearToken::from_yoctonear(yocto));
    }

    builder
}

fn new_vault_with_liquidity() -> Vault {
    let mut vault = Vault::new(owner(), 0, 1);
    vault.liquidity_request = Some(create_valid_liquidity_request("usdc.test.near".parse().unwrap()));
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at: 0,
    });
    vault
}

fn initialise_liquidation_context(balance_near: u128, attached_deposit: Option<u128>, epoch: u64) -> Vault {
    let now = expiry_timestamp();
    let builder = context_builder(balance_near, attached_deposit, now, epoch);
    testing_env!(builder.build());

    new_vault_with_liquidity()
}

fn set_env_with_promise_result(builder: VMContextBuilder, result: PromiseResult) {
    set_env_with_promise_results(builder, vec![result]);
}

fn set_env_with_promise_results(builder: VMContextBuilder, results: Vec<PromiseResult>) {
    let context = builder.build();
    testing_env!(context, test_vm_config(), RuntimeFeesConfig::test(), Default::default(), results);
}

fn insert_unstake_entry(vault: &mut Vault, validator: &str, amount: u128, epoch_height: u64) {
    let validator_id: AccountId = validator.parse().unwrap();
    vault.unstake_entries.insert(
        &validator_id,
        &UnstakeEntry {
            amount,
            epoch_height,
        },
    );
    vault.active_validators.insert(&validator_id);
}

fn enable_active_validator(vault: &mut Vault, validator: &str) {
    vault
        .active_validators
        .insert(&validator.parse::<AccountId>().unwrap());
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn process_claims_requires_one_yocto() {
    let builder = context_builder(10, None, expiry_timestamp(), 100);
    testing_env!(builder.build());

    let mut vault = new_vault_with_liquidity();
    vault.process_claims();
}

#[test]
#[should_panic(expected = "No accepted offer found")]
fn process_claims_requires_offer() {
    let builder = context_builder(10, Some(1), expiry_timestamp(), 100);
    testing_env!(builder.build());

    let mut vault = Vault::new(owner(), 0, 1);
    vault.liquidity_request = Some(create_valid_liquidity_request("usdc.test.near".parse().unwrap()));
    vault.process_claims();
}

#[test]
#[should_panic(expected = "Liquidation not allowed until")]
fn process_claims_rejects_before_expiry() {
    let now = 1;
    let builder = context_builder(10, Some(1), now, 100);
    testing_env!(builder.build());

    let mut vault = new_vault_with_liquidity();
    vault.process_claims();
}

#[test]
fn process_claims_initialises_liquidation_and_lock() {
    let mut vault = initialise_liquidation_context(0, Some(1), 200);

    assert!(vault.liquidation.is_none());
    enable_active_validator(&mut vault, VALIDATOR_A);

    let _promise = vault.process_claims();

    assert!(vault.liquidation.is_some());
    assert_eq!(vault.processing_state, ProcessingState::ProcessClaims);
    assert!(vault.processing_since > 0);
}

#[test]
#[should_panic(expected = "Vault busy with ProcessClaims")]
fn process_claims_rejects_during_active_lock() {
    let mut vault = initialise_liquidation_context(0, Some(1), 200);
    enable_active_validator(&mut vault, VALIDATOR_A);

    vault.process_claims();
    vault.process_claims();
}

#[test]
fn process_claims_allows_reentry_after_timeout() {
    let mut vault = initialise_liquidation_context(0, Some(1), 200);
    enable_active_validator(&mut vault, VALIDATOR_A);

    vault.process_claims();

    let now = expiry_timestamp() + LOCK_TIMEOUT + 5;
    let mut builder = context_builder(0, Some(1), now, 200);
    builder.predecessor_account_id(owner());
    testing_env!(builder.build());

    vault.processing_since = now - LOCK_TIMEOUT - 1;
    vault.process_claims();

    assert_eq!(vault.processing_state, ProcessingState::ProcessClaims);
}

#[test]
fn process_claims_waits_when_maturing_covers_debt() {
    let mut vault = initialise_liquidation_context(0, Some(1), 200);
    insert_unstake_entry(
        &mut vault,
        VALIDATOR_A,
        5 * YOCTO_NEAR,
        env::epoch_height(), // still maturing
    );

    let _promise = vault.process_claims();

    assert!(vault.liquidation.is_some());
    assert_eq!(vault.processing_state, ProcessingState::Idle);
}

#[test]
fn process_claims_prioritises_matured_unstake() {
    let epoch = 400;
    let mut vault = initialise_liquidation_context(0, Some(1), epoch);
    insert_unstake_entry(
        &mut vault,
        VALIDATOR_A,
        3 * YOCTO_NEAR,
        epoch - NUM_EPOCHS_TO_UNLOCK,
    );

    let _ = vault.process_claims();

    let receipts = get_created_receipts();
    assert!(
        contains_function_call(&receipts, "withdraw_all"),
        "should withdraw matured stake before other steps"
    );
    assert!(
        contains_function_call(&receipts, "on_batch_claim_unstaked"),
        "callback must be scheduled for matured claim processing"
    );
}

#[test]
fn process_claims_queries_additional_unstake_when_shortfall() {
    let epoch = 210;
    let mut vault = initialise_liquidation_context(10, Some(1), epoch);
    enable_active_validator(&mut vault, VALIDATOR_A);
    vault
        .liquidity_request
        .as_mut()
        .unwrap()
        .collateral = NearToken::from_near(100);

    let _ = vault.process_claims();

    let receipts = get_created_receipts();
    assert!(
        contains_function_call(&receipts, "get_account_staked_balance"),
        "insufficient liquid funds should trigger validator queries"
    );
    assert!(
        contains_function_call(&receipts, "on_total_staked_process_claims"),
        "validator queries should chain into their callback"
    );
}

#[test]
fn process_claims_pays_lender_when_liquid_balance_sufficient() {
    let epoch = 300;
    let mut vault = initialise_liquidation_context(200, Some(1), epoch);
    let lender = alice();
    vault
        .liquidity_request
        .as_mut()
        .unwrap()
        .collateral = NearToken::from_near(2);

    let _ = vault.process_claims();

    let receipts = get_created_receipts();
    assert!(
        contains_function_call(&receipts, "on_lender_payout_complete"),
        "direct payout must attach its completion callback"
    );
    let transfer = find_transfer_to(&receipts, &lender).expect("transfer to lender expected");
    assert_eq!(transfer.as_yoctonear(), 2 * YOCTO_NEAR);
}

#[test]
fn on_lender_payout_complete_finalises_liquidation() {
    let mut vault = initialise_liquidation_context(5, None, 200);
    vault.processing_state = ProcessingState::ProcessClaims;
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });

    let _ = vault.on_lender_payout_complete(alice(), 5 * YOCTO_NEAR, true, Ok(()));

    assert!(vault.liquidity_request.is_none());
    assert!(vault.accepted_offer.is_none());
    assert!(vault.liquidation.is_none());
    assert_eq!(vault.processing_state, ProcessingState::Idle);
}

#[test]
fn on_lender_payout_complete_records_partial_amount() {
    let mut vault = initialise_liquidation_context(2, None, 200);
    vault.processing_state = ProcessingState::ProcessClaims;
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });

    let _ = vault.on_lender_payout_complete(alice(), 2 * YOCTO_NEAR, false, Ok(()));

    let liquidated = vault
        .liquidation
        .as_ref()
        .expect("liquidation state should remain for partial payout")
        .liquidated
        .as_yoctonear();
    assert_eq!(liquidated, 2 * YOCTO_NEAR);
    assert!(vault.liquidity_request.is_some());
    assert!(vault.accepted_offer.is_some());
    assert_eq!(vault.processing_state, ProcessingState::Idle);
}

fn promise_success_result(value: &[u8]) -> PromiseResult {
    PromiseResult::Successful(value.to_vec())
}

fn contains_function_call(receipts: &[Receipt], method: &str) -> bool {
    receipts.iter().flat_map(|r| r.actions.iter()).any(|action| match action {
        MockAction::FunctionCallWeight { method_name, .. } => method_name == method.as_bytes(),
        _ => false,
    })
}

fn find_transfer_to(receipts: &[Receipt], receiver: &AccountId) -> Option<NearToken> {
    receipts.iter().find(|receipt| &receipt.receiver_id == receiver).and_then(|receipt| {
        receipt.actions.iter().find_map(|action| match action {
            MockAction::Transfer { deposit, .. } => Some(*deposit),
            _ => None,
        })
    })
}

#[test]
fn on_batch_claim_unstaked_removes_successful_entries() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(0, Some(1), epoch);
    insert_unstake_entry(
        &mut vault,
        VALIDATOR_A,
        3 * YOCTO_NEAR,
        epoch - NUM_EPOCHS_TO_UNLOCK,
    );
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let builder = context_builder(0, Some(1), expiry_timestamp(), epoch);
    set_env_with_promise_result(builder, promise_success_result(&[]));

    let validator: AccountId = VALIDATOR_A.parse().unwrap();
    let _ = vault.on_batch_claim_unstaked(vec![validator.clone()]);

    assert!(
        vault.unstake_entries.get(&validator).is_none(),
        "validator entry should be removed after successful claim"
    );
}

#[test]
fn on_batch_claim_unstaked_preserves_failed_entries() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(0, Some(1), epoch);
    insert_unstake_entry(
        &mut vault,
        VALIDATOR_A,
        3 * YOCTO_NEAR,
        epoch - NUM_EPOCHS_TO_UNLOCK,
    );
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let builder = context_builder(0, Some(1), expiry_timestamp(), epoch);
    set_env_with_promise_result(builder, PromiseResult::Failed);

    let validator: AccountId = VALIDATOR_A.parse().unwrap();
    let _ = vault.on_batch_claim_unstaked(vec![validator.clone()]);

    assert!(
        vault.unstake_entries.get(&validator).is_some(),
        "validator entry should remain when withdraw fails"
    );
}

#[test]
fn on_total_staked_process_claims_waits_when_deficit_is_zero() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(0, None, epoch);
    insert_unstake_entry(
        &mut vault,
        VALIDATOR_A,
        5 * YOCTO_NEAR,
        epoch, // still maturing
    );
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let builder = context_builder(0, None, expiry_timestamp(), epoch);
    set_env_with_promise_result(builder, promise_success_result(&serde_json::to_vec(&U128(0)).unwrap()));

    let _ = vault.on_total_staked_process_claims(vec![VALIDATOR_A.parse().unwrap()]);

    assert_eq!(vault.processing_state, ProcessingState::Idle);
}

#[test]
fn on_total_staked_process_claims_removes_zero_balance_validator() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(0, None, epoch);
    enable_active_validator(&mut vault, VALIDATOR_A);
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let builder = context_builder(0, None, expiry_timestamp(), epoch);
    set_env_with_promise_result(builder, promise_success_result(&serde_json::to_vec(&U128(0)).unwrap()));

    let validator: AccountId = VALIDATOR_A.parse().unwrap();
    let _ = vault.on_total_staked_process_claims(vec![validator.clone()]);

    assert!(
        !vault.active_validators.contains(&validator),
        "validator with zero stake should be removed"
    );
}

#[test]
fn on_total_staked_process_claims_schedules_unstake_for_deficit() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(0, None, epoch);
    enable_active_validator(&mut vault, VALIDATOR_A);
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let builder = context_builder(0, None, expiry_timestamp(), epoch);
    set_env_with_promise_result(
        builder,
        promise_success_result(&serde_json::to_vec(&U128(3 * YOCTO_NEAR)).unwrap()),
    );

    let _ = vault.on_total_staked_process_claims(vec![VALIDATOR_A.parse().unwrap()]);

    assert!(
        vault.processing_state == ProcessingState::ProcessClaims,
        "processing lock should remain held while unstake promise executes"
    );
}

#[test]
fn on_total_staked_process_claims_handles_failed_queries() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(0, None, epoch);
    enable_active_validator(&mut vault, VALIDATOR_A);
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let builder = context_builder(0, None, expiry_timestamp(), epoch);
    set_env_with_promise_results(builder, vec![PromiseResult::Failed]);

    let validator: AccountId = VALIDATOR_A.parse().unwrap();
    let _ = vault.on_total_staked_process_claims(vec![validator.clone()]);

    assert_eq!(vault.processing_state, ProcessingState::Idle);
    assert!(
        vault.active_validators.contains(&validator),
        "validator should remain active when query fails"
    );
}

#[test]
fn on_total_staked_process_claims_ignores_invalid_payloads() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(0, None, epoch);
    enable_active_validator(&mut vault, VALIDATOR_A);
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let builder = context_builder(0, None, expiry_timestamp(), epoch);
    let invalid_payload = serde_json::to_vec(&serde_json::json!("not-a-number")).unwrap();
    set_env_with_promise_results(builder, vec![promise_success_result(&invalid_payload)]);

    let validator: AccountId = VALIDATOR_A.parse().unwrap();
    let _ = vault.on_total_staked_process_claims(vec![validator.clone()]);

    assert_eq!(vault.processing_state, ProcessingState::Idle);
    assert!(
        vault.active_validators.contains(&validator),
        "validator should remain active when payload cannot be parsed"
    );
}

#[test]
fn on_batch_unstake_updates_entries_on_success() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(0, None, epoch);
    enable_active_validator(&mut vault, VALIDATOR_A);
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let builder = context_builder(0, None, expiry_timestamp(), epoch);
    set_env_with_promise_result(builder, promise_success_result(&[]));

    let validator: AccountId = VALIDATOR_A.parse().unwrap();
    let entries = vec![(validator.clone(), 2 * YOCTO_NEAR, true)];
    let _ = vault.on_batch_unstake(entries.clone());

    assert!(
        vault.unstake_entries.get(&validator).is_some(),
        "unstake entry should be recorded"
    );
    assert!(
        !vault.active_validators.contains(&validator),
        "validator removed after unstaking entire balance"
    );
}

#[test]
fn on_batch_unstake_keeps_validator_for_partial_unstake() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(0, None, epoch);
    enable_active_validator(&mut vault, VALIDATOR_A);
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let builder = context_builder(0, None, expiry_timestamp(), epoch);
    set_env_with_promise_result(builder, promise_success_result(&[]));

    let validator: AccountId = VALIDATOR_A.parse().unwrap();
    let entries = vec![(validator.clone(), 2 * YOCTO_NEAR, false)];
    let _ = vault.on_batch_unstake(entries.clone());

    let recorded = vault
        .unstake_entries
        .get(&validator)
        .expect("unstake entry should be created");
    assert_eq!(recorded.amount, 2 * YOCTO_NEAR);
    assert_eq!(recorded.epoch_height, epoch);
    assert!(
        vault.active_validators.contains(&validator),
        "validator should remain active when stake is only partially removed"
    );
    assert_eq!(
        vault.processing_state,
        ProcessingState::Idle,
        "lock released when no payout is attempted"
    );
}

#[test]
fn on_batch_unstake_handles_failed_promises() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(0, None, epoch);
    enable_active_validator(&mut vault, VALIDATOR_A);
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let builder = context_builder(0, None, expiry_timestamp(), epoch);
    set_env_with_promise_result(builder, PromiseResult::Failed);

    let validator: AccountId = VALIDATOR_A.parse().unwrap();
    let entries = vec![(validator.clone(), 2 * YOCTO_NEAR, true)];
    let _ = vault.on_batch_unstake(entries);

    assert!(
        vault.unstake_entries.get(&validator).is_none(),
        "failed unstake should not create entries"
    );
    assert!(
        vault.active_validators.contains(&validator),
        "validator should remain active after failure"
    );
    assert_eq!(vault.processing_state, ProcessingState::Idle);
}

#[test]
fn on_batch_unstake_attempts_payout_when_liquid_balance_exists() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(1000, None, epoch);
    enable_active_validator(&mut vault, VALIDATOR_A);
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let builder = context_builder(1000, None, expiry_timestamp(), epoch);
    set_env_with_promise_result(builder, promise_success_result(&[]));

    let validator: AccountId = VALIDATOR_A.parse().unwrap();
    let entries = vec![(validator.clone(), YOCTO_NEAR, true)];
    let _ = vault.on_batch_unstake(entries);

    assert!(
        vault.unstake_entries.get(&validator).is_some(),
        "successful unstake should update entries"
    );
    assert_eq!(
        vault.processing_state,
        ProcessingState::ProcessClaims,
        "lock remains held while payout promise is in flight"
    );
}

#[test]
fn lender_payout_failure_retains_state() {
    let epoch = 200;
    let mut vault = initialise_liquidation_context(5, None, epoch);
    vault.liquidation = Some(Liquidation { liquidated: NearToken::from_yoctonear(0) });
    vault.processing_state = ProcessingState::ProcessClaims;

    let _ = vault.on_lender_payout_complete(alice(), YOCTO_NEAR, false, Err(near_sdk::PromiseError::Failed));

    assert!(vault.liquidation.is_some());
    assert_eq!(vault.processing_state, ProcessingState::Idle);
}
