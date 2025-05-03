use near_sdk::json_types::U128;
use near_sdk::NearToken;
use near_workspaces::{network::Sandbox, Account, Contract, Worker};
use serde_json::json;
use test_utils::{
    create_test_validator, initialize_test_vault_on_sub_account, setup_sandbox_and_accounts,
    RefundEntry, VaultViewState, VAULT_CALL_GAS,
};

#[path = "test_utils.rs"]
mod test_utils;

pub async fn simulate_failed_claim_vault(
) -> anyhow::Result<(Worker<Sandbox>, Contract, Account, Account, Account)> {
    // Setup sandbox, root (old_owner), and buyer
    let (worker, root, buyer) = setup_sandbox_and_accounts().await?;

    // Create a vault owner that is not root
    let owner = root
        .create_subaccount("owner")
        .initial_balance(NearToken::from_near(5))
        .transact()
        .await?
        .into_result()?;

    // Initialize vault under root account
    let vault = initialize_test_vault_on_sub_account(&root).await?.contract;

    // Transfer vault to owner
    root.call(vault.id(), "transfer_ownership")
        .args_json(json!({ "new_owner": owner.id() }))
        .gas(VAULT_CALL_GAS)
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    // Fetch storage cost
    let storage_cost: U128 = vault.view("view_storage_cost").await?.json()?;

    // List vault for takeover
    owner
        .call(vault.id(), "list_for_takeover")
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    // Delete the old owner account to simulate refund failure
    worker
        .delete_account(owner.id(), owner.signer(), root.id())
        .await?
        .into_result()?;

    // Claim vault from buyer — refund should fail
    let res = buyer
        .call(vault.id(), "claim_vault")
        .deposit(NearToken::from_yoctonear(storage_cost.0))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Extract logs from the transaction
    let logs = res.logs().join("\n");

    // Look for the refund_failed event emitted inside the failed callback
    assert!(
        logs.contains(r#""event":"claim_vault_failed""#),
        "Expected refund_failed log in transaction logs. Got:\n{}",
        logs
    );

    // Vault should still have original owner and still listed for takeover
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert_eq!(
        state.owner,
        owner.id().to_string(),
        "Owner should not change"
    );
    assert!(state.is_listed_for_takeover, "Vault should still be listed");

    // Fetch refund list from the contract
    let refunds: Vec<(u64, RefundEntry)> = vault.view("get_all_refund_entries").await?.json()?;

    // There should be exactly 1 refund entry recorded
    assert_eq!(refunds.len(), 1, "Expected one refund entry");

    // Inspect the refund entry
    let (_, refund) = &refunds[0];
    assert_eq!(refund.token, None, "Expected native NEAR refund");
    assert_eq!(
        &refund.proposer,
        buyer.id(),
        "Refund should go to the buyer"
    );
    assert_eq!(
        refund.amount.0, storage_cost.0,
        "Refund amount should match attached storage cost"
    );

    Ok((worker, vault, root, owner, buyer))
}

#[tokio::test]
async fn test_claim_vault_fallback_when_old_owner_deleted() -> anyhow::Result<()> {
    // Simulate failed claim_vault
    let (_, vault, _, _, buyer) = simulate_failed_claim_vault().await?;

    // Get buyer balance before refund
    let balance_before = buyer.view_account().await?.balance;

    // Retry the refund from the buyer account
    let retry_result = buyer
        .call(vault.id(), "retry_refunds")
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Refund list should now be empty
    let refunds_after: Vec<(u64, RefundEntry)> =
        vault.view("get_all_refund_entries").await?.json()?;
    assert!(
        refunds_after.is_empty(),
        "Refund list should be empty after retry"
    );

    // Logs should include a successful refund event
    let logs = retry_result.logs();
    let log_str = logs.join("\n");
    assert!(
        log_str.contains(r#""event":"retry_refund_succeeded""#),
        "Expected retry_refund_succeeded event in logs. Got: {log_str}"
    );

    // Get buyer balance after refund
    let balance_after = buyer.view_account().await?.balance;

    assert!(
        balance_after > balance_before,
        "Expected the buyer's balance to increase",
    );

    Ok(())
}

#[tokio::test]
async fn test_delegate_should_fail_if_refund_list_is_not_empty() -> anyhow::Result<()> {
    let (worker, vault, root, _, _) = simulate_failed_claim_vault().await?;

    // We recreate the vault owner account as it was deleted earlier
    // so we can simulate the claim_vault failure
    // Create a vault owner that is not root
    let owner = root
        .create_subaccount("owner")
        .initial_balance(NearToken::from_near(5))
        .transact()
        .await?
        .into_result()?;

    // Create a vaidator
    let validator = create_test_validator(&worker, &root).await?;

    // Attempt to delegate when there are pending refunds
    let result = owner
        .call(vault.id(), "delegate")
        .args_json(json!({
            "validator": validator.id(),
            "amount": NearToken::from_near(1)
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Assert delegate call failed
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Cannot delegate while there are pending refund entries"),
        "Expected failure due to pending refund entries, got: {failure_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_withdraw_should_fail_if_refund_list_is_not_empty() -> anyhow::Result<()> {
    let (_, vault, root, _, _) = simulate_failed_claim_vault().await?;

    // Recreate the deleted vault owner
    let owner = root
        .create_subaccount("owner")
        .initial_balance(NearToken::from_near(5))
        .transact()
        .await?
        .into_result()?;

    // Attempt to withdraw NEAR while refund_list is not empty
    let amount = near_sdk::NearToken::from_near(1);
    let result = owner
        .call(vault.id(), "withdraw_balance")
        .args_json(serde_json::json!({
            "token_address": null,
            "amount": amount.as_yoctonear().to_string(),
            "to": root.id()
        }))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    // Expect failure due to pending refund entries
    let failure_text = format!("{:?}", result.failures());
    assert!(
        failure_text.contains("Cannot withdraw while there are pending refund entries"),
        "Expected failure due to pending refund entries, got: {failure_text}"
    );

    Ok(())
}
