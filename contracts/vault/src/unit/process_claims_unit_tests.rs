use near_sdk::{env, json_types::U128, testing_env, AccountId, NearToken};
use test_utils::{
    alice, create_valid_liquidity_request, get_context, get_context_with_timestamp, owner,
    YOCTO_NEAR,
};

use crate::{
    contract::Vault,
    types::{
        AcceptedOffer, Liquidation, PendingLiquidityRequest, ProcessingState, UnstakeEntry,
        LOCK_TIMEOUT, NUM_EPOCHS_TO_UNLOCK,
    },
};

#[path = "test_utils.rs"]
mod test_utils;

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn test_process_claims_requires_one_yocto() {
    // Set up the test context without attaching 1 yoctoNEAR
    let context = get_context(owner(), NearToken::from_near(10), None);
    testing_env!(context);

    // Create a new vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to call `process_claims` without 1 yocto (should panic)
    vault.process_claims();
}

#[test]
#[should_panic(expected = "No accepted offer found")]
fn test_process_claims_fails_if_no_accepted_offer() {
    // Set up test context with vault owner and attach 1 yoctoNEAR
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Initialize a fresh vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Attempt to call process_claims without accepted_offer set
    // This should panic with "No accepted offer found"
    vault.process_claims();
}

#[test]
#[should_panic(expected = "Liquidation not allowed until")]
fn test_process_claims_fails_if_not_expired() {
    // Set up test context with vault owner and 1 yoctoNEAR attached
    let context = get_context(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
    );
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Insert a valid liquidity request with duration = 86400 seconds (1 day)
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.test.near".parse().unwrap(),
    ));

    // Simulate an accepted_offer just a few seconds ago (e.g., now - 1 second)
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at: env::block_timestamp(),
    });

    // Call process_claims — should panic because not enough time has passed
    vault.process_claims();
}

#[test]
fn test_process_claims_starts_liquidation_after_expiry() {
    // Simulate accepted_at = 1_000_000_000 (1s), and duration = 86400s (1 day)
    let accepted_at = 1_000_000_000;
    let duration_secs = 86400;
    let expiry_timestamp = accepted_at + (duration_secs * 1_000_000_000);
    let now = expiry_timestamp + 1;

    // Set up test context with vault owner and 1 yoctoNEAR attached
    // Vault only has 2 NEAR, which is less than the 5 NEAR collateral
    let context = get_context_with_timestamp(
        owner(),
        NearToken::from_near(2),
        Some(NearToken::from_yoctonear(1)),
        Some(now),
    );
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Insert one dummy validator
    vault
        .active_validators
        .insert(&"validator1.testnet".parse().unwrap());

    // Insert a valid liquidity request with 1-day duration
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.test.near".parse().unwrap(),
    ));

    // Simulate an accepted offer with a timestamp older than expiration
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at,
    });

    // Assert that liquidation is not yet initialized
    assert!(vault.liquidation.is_none());

    // Call process_claims — should initialize liquidation and begin processing
    let _ = vault.process_claims();

    // Assert that liquidation has been initialized
    assert!(vault.liquidation.is_some());

    // Assert that the processing_claims flag is true (lock acquired)
    assert_eq!(vault.processing_state, ProcessingState::ProcessClaims);

    // Assert that the liquidation was not finalized (liquidity_request is still Some)
    assert!(vault.liquidity_request.is_some());
}

#[test]
#[should_panic(expected = "Vault busy with ProcessClaims")]
fn test_process_claims_fails_if_locked_and_not_expired() {
    // Simulate accepted_at = 1_000_000_000 (1s), and duration = 86400s (1 day)
    let accepted_at = 1_000_000_000;
    let duration_secs = 86400;
    let expiry_timestamp = accepted_at + (duration_secs * 1_000_000_000);
    let now = expiry_timestamp + 1;

    // Set up test context with vault owner and 1 yoctoNEAR attached
    let context = get_context_with_timestamp(
        owner(),
        NearToken::from_near(2),
        Some(NearToken::from_yoctonear(1)),
        Some(now),
    );
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Insert one dummy validator
    vault
        .active_validators
        .insert(&"validator1.testnet".parse().unwrap());

    // Insert a valid liquidity request with 1-day duration
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.test.near".parse().unwrap(),
    ));

    // Simulate an accepted offer with a timestamp older than expiration
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at,
    });

    // Call process_claims — should initialize liquidation and begin processing
    vault.process_claims();

    // Try to calling process_claims again should panic
    vault.process_claims();
}

#[test]
fn test_process_claims_allows_reentry_after_lock_timeout() {
    // Simulate accepted_at = 1_000_000_000 (1s), and duration = 86400s (1 day)
    let accepted_at = 1_000_000_000;
    let duration_secs = 86400;
    let expiry_timestamp = accepted_at + (duration_secs * 1_000_000_000);

    // Simulate a point in time long after expiration + LOCK_TIMEOUT
    let now = expiry_timestamp + LOCK_TIMEOUT + 1;

    // Set up test context with vault owner and 1 yoctoNEAR attached
    let context = get_context_with_timestamp(
        owner(),
        NearToken::from_near(2),
        Some(NearToken::from_yoctonear(1)),
        Some(now),
    );
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Insert one dummy validator
    vault
        .active_validators
        .insert(&"validator1.testnet".parse().unwrap());

    // Insert a valid liquidity request
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.test.near".parse().unwrap(),
    ));

    // Insert an accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at,
    });

    // Simulate an old lock (stale by more than LOCK_TIMEOUT)
    vault.processing_state = ProcessingState::ProcessClaims;
    vault.processing_since = now - LOCK_TIMEOUT - 1;

    // Call process_claims — should succeed by clearing stale lock and proceeding
    let _ = vault.process_claims();

    // Assert that the lock is active again
    assert_eq!(vault.processing_state, ProcessingState::ProcessClaims);

    // Assert that liquidation has been initialized
    assert!(vault.liquidation.is_some());
}

#[test]
fn test_process_claims_fulfills_full_repayment_if_balance_sufficient() {
    // Simulate accepted_at = 1_000_000_000 (1s), and duration = 86400s (1 day)
    let accepted_at = 1_000_000_000;
    let duration_secs = 86400;
    let expiry_timestamp = accepted_at + (duration_secs * 1_000_000_000);
    let now = expiry_timestamp + 1;

    // Set up test context with vault owner and 1 yoctoNEAR attached
    // Vault has enough balance to cover full collateral (5 NEAR)
    let context = get_context_with_timestamp(
        owner(),
        NearToken::from_near(10),
        Some(NearToken::from_yoctonear(1)),
        Some(now),
    );
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Add one dummy validator
    vault
        .active_validators
        .insert(&"validator1.testnet".parse().unwrap());

    // Insert a valid liquidity request with 5 NEAR collateral
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.test.near".parse().unwrap(),
    ));

    // Insert an accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at,
    });

    // Assert initial state before claim
    assert!(vault.liquidation.is_none());

    // Call process_claims — should finalize via callback once transfer settles
    let _ = vault.process_claims();

    // Simulate the successful transfer callback to finish liquidation
    let payout = vault
        .liquidity_request
        .as_ref()
        .unwrap()
        .collateral
        .as_yoctonear();
    vault.on_lender_payout_complete(alice(), payout, true, Ok(()));

    // Assert liquidation state is cleared (repayment complete)
    assert!(vault.liquidation.is_none());
    assert!(vault.liquidity_request.is_none());
    assert!(vault.accepted_offer.is_none());
    assert_eq!(vault.processing_state, ProcessingState::Idle);
}

#[test]
fn test_process_claims_does_partial_repayment_if_insufficient_balance() {
    // Simulate accepted_at = 1_000_000_000 (1s), and duration = 86400s (1 day)
    let accepted_at = 1_000_000_000;
    let duration_secs = 86400;
    let expiry_timestamp = accepted_at + (duration_secs * 1_000_000_000);
    let now = expiry_timestamp + 1;

    // Set up test context with vault owner and 1 yoctoNEAR attached
    // Vault has only 2 NEAR, less than the 5 NEAR required collateral
    let mut context = get_context_with_timestamp(
        owner(),
        NearToken::from_near(2),
        Some(NearToken::from_yoctonear(1)),
        Some(now),
    );
    context.storage_usage = 0u64;
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Add one dummy validator
    vault
        .active_validators
        .insert(&"validator1.testnet".parse().unwrap());

    // Insert a valid liquidity request (5 NEAR collateral)
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.test.near".parse().unwrap(),
    ));

    // Insert an accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at,
    });

    // Assert liquidation not started yet
    assert!(vault.liquidation.is_none());

    // Call process_claims — should do partial transfer and proceed
    let _ = vault.process_claims();

    // Assert liquidation is now active
    assert!(vault.liquidation.is_some());

    // Assert liquidation is not complete
    assert!(vault.liquidity_request.is_some());
    assert!(vault.accepted_offer.is_some());

    // Assert processing lock is acquired
    assert_eq!(vault.processing_state, ProcessingState::ProcessClaims);

    // Assert that something was transferred (liquidated > 0)
    let repaid = vault
        .liquidation
        .as_ref()
        .unwrap()
        .liquidated
        .as_yoctonear();
    assert!(repaid > 0 && repaid < 5 * YOCTO_NEAR);
}

#[test]
fn test_process_claims_handles_matured_unstaked_entries() {
    // Simulate accepted_at = 1_000_000_000 (1s), and duration = 86400s (1 day)
    let accepted_at = 1_000_000_000;
    let duration_secs = 86400;
    let expiry_timestamp = accepted_at + (duration_secs * 1_000_000_000);
    let now = expiry_timestamp + 1;

    // Set up test context with 0 NEAR balance and 1 yoctoNEAR attached
    let context = get_context_with_timestamp(
        owner(),
        NearToken::from_near(0),
        Some(NearToken::from_yoctonear(1)),
        Some(now),
    );
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Insert dummy validator
    let validator: AccountId = "validator1.testnet".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Insert valid liquidity request
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.test.near".parse().unwrap(),
    ));

    // Insert accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at,
    });

    // Insert a matured unstake entry (epoch_height + 4 has passed)
    vault.unstake_entries.insert(
        &validator,
        &UnstakeEntry {
            amount: 3 * YOCTO_NEAR,
            epoch_height: env::epoch_height() - NUM_EPOCHS_TO_UNLOCK,
        },
    );

    // Call process_claims — it should recognize matured unstake and proceed
    let _ = vault.process_claims();

    // Assert liquidation started
    assert!(vault.liquidation.is_some());

    // Assert lock is acquired
    assert_eq!(vault.processing_state, ProcessingState::ProcessClaims);

    // Assert liquidity request is not cleared (repayment incomplete)
    assert!(vault.liquidity_request.is_some());

    // Assert unstake entry still exists (withdraw_all runs async)
    assert!(vault.unstake_entries.get(&validator).is_some());
}

#[test]
fn test_process_claims_waits_if_enough_is_maturing() {
    // Simulate accepted_at = 1_000_000_000 (1s), and duration = 86400s (1 day)
    let accepted_at = 1_000_000_000;
    let duration_secs = 86400;
    let expiry_timestamp = accepted_at + (duration_secs * 1_000_000_000);
    let now = expiry_timestamp + 1;

    // Set up test context with 0 NEAR balance and 1 yoctoNEAR attached
    let context = get_context_with_timestamp(
        owner(),
        NearToken::from_near(0),
        Some(NearToken::from_yoctonear(1)),
        Some(now),
    );
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Insert dummy validator
    let validator: AccountId = "validator1.testnet".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Insert a valid liquidity request with 5 NEAR collateral
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.test.near".parse().unwrap(),
    ));

    // Insert an accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at,
    });

    // Insert an unstake entry that is still maturing (epoch_height < unlock threshold)
    vault.unstake_entries.insert(
        &validator,
        &UnstakeEntry {
            amount: 5 * YOCTO_NEAR,
            epoch_height: env::epoch_height(),
        },
    );

    // Call process_claims — should log wait and not panic
    let _ = vault.process_claims();

    // Assert liquidation is initialized
    assert!(vault.liquidation.is_some());

    // Assert lock is released (since we’re just waiting)
    assert_eq!(vault.processing_state, ProcessingState::Idle);

    // Assert liquidity request is still active
    assert!(vault.liquidity_request.is_some());

    // Assert no unstake entry has been cleared (not matured)
    assert!(vault.unstake_entries.get(&validator).is_some());
}

#[test]
fn test_process_claims_triggers_unstake_if_maturing_insufficient() {
    // Simulate accepted_at = 1_000_000_000 (1s), and duration = 86400s (1 day)
    let accepted_at = 1_000_000_000;
    let duration_secs = 86400;
    let expiry_timestamp = accepted_at + (duration_secs * 1_000_000_000);
    let now = expiry_timestamp + 1;

    // Set up test context with 0 NEAR balance and 1 yoctoNEAR attached
    let context = get_context_with_timestamp(
        owner(),
        NearToken::from_near(0),
        Some(NearToken::from_yoctonear(1)),
        Some(now),
    );
    testing_env!(context);

    // Create a vault instance
    let mut vault = Vault::new(owner(), 0, 1);

    // Insert dummy validator
    let validator: AccountId = "validator1.testnet".parse().unwrap();
    vault.active_validators.insert(&validator);

    // Insert a valid liquidity request with 5 NEAR collateral
    vault.liquidity_request = Some(create_valid_liquidity_request(
        "usdc.test.near".parse().unwrap(),
    ));

    // Insert an accepted offer
    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at,
    });

    // Insert an unstake entry that's still maturing (but not enough to cover debt)
    vault.unstake_entries.insert(
        &validator,
        &UnstakeEntry {
            amount: 1 * YOCTO_NEAR,
            epoch_height: env::epoch_height(),
        },
    );

    // Call process_claims — should proceed to batch_query_total_staked()
    let _ = vault.process_claims();

    // Assert liquidation is initialized
    assert!(vault.liquidation.is_some());

    // Assert lock is still held — async callback expected
    assert_eq!(vault.processing_state, ProcessingState::ProcessClaims);

    // Assert liquidity request is not cleared
    assert!(vault.liquidity_request.is_some());

    // Assert unstake entry still exists
    assert!(vault.unstake_entries.get(&validator).is_some());
}

#[test]
fn test_on_lender_payout_complete_clears_state() {
    let now = 1_000_000_500;
    let context =
        get_context_with_timestamp(owner(), NearToken::from_near(10), None, Some(now));
    testing_env!(context);

    let lender = alice();
    let mut vault = Vault::new(owner(), 0, 1);

    vault.processing_state = ProcessingState::ProcessClaims;
    vault.processing_since = now;

    vault.liquidity_request =
        Some(create_valid_liquidity_request("usdc.test.near".parse().unwrap()));
    vault.accepted_offer = Some(AcceptedOffer {
        lender: lender.clone(),
        accepted_at: now - 1,
    });
    vault.liquidation = Some(Liquidation {
        liquidated: NearToken::from_yoctonear(0),
    });
    vault.pending_liquidity_request = Some(PendingLiquidityRequest {
        token: "usdc.test.near".parse().unwrap(),
        amount: U128(1_000_000),
        interest: U128(100_000),
        collateral: NearToken::from_near(5),
        duration: 86400,
    });

    vault.on_lender_payout_complete(lender.clone(), 100, true, Ok(()));

    assert!(vault.liquidity_request.is_none(), "Request should be cleared");
    assert!(vault.accepted_offer.is_none(), "Offer should be cleared");
    assert!(vault.liquidation.is_none(), "Liquidation should be cleared");
    assert!(
        vault.pending_liquidity_request.is_none(),
        "Pending request should be cleared"
    );
    assert_eq!(vault.processing_state, ProcessingState::Idle);
    assert_eq!(vault.processing_since, 0);
}

#[test]
#[should_panic(expected = "Loan duration exceeds supported range")]
fn test_process_claims_duration_overflow_panics() {
    let now = 1_000_000_500;
    let context = get_context_with_timestamp(
        owner(),
        NearToken::from_near(0),
        Some(NearToken::from_yoctonear(1)),
        Some(now),
    );
    testing_env!(context);

    let mut vault = Vault::new(owner(), 0, 1);
    vault.active_validators.insert(&"validator1.testnet".parse().unwrap());

    let mut request = create_valid_liquidity_request("usdc.test.near".parse().unwrap());
    request.duration = u64::MAX;
    vault.liquidity_request = Some(request);

    vault.accepted_offer = Some(AcceptedOffer {
        lender: alice(),
        accepted_at: 0,
    });

    vault.process_claims();
}
