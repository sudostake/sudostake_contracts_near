use near_sdk::{json_types::U128, NearToken};
use test_utils::{setup_sandbox_and_accounts, VaultViewState, VAULT_CALL_GAS};

#[path = "test_utils.rs"]
mod test_utils;

#[tokio::test]
async fn test_claim_vault_success_transfers_ownership() -> anyhow::Result<()> {
    let (_, root, new_vault_owner) = setup_sandbox_and_accounts().await?;
    let vault = test_utils::initialize_test_vault_on_sub_account(&root)
        .await?
        .contract;

    // Get storage cost
    let storage_cost: U128 = vault
        .view("view_storage_cost")
        .await?
        .json()
        .expect("Expected U128");

    // List vault for takeover
    root.call(vault.id(), "list_for_takeover")
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    // Record root's initial balance
    let initial_root_balance = root.view_account().await?.balance.as_yoctonear();

    // Claim the vault with exact storage cost
    let outcome = new_vault_owner
        .call(vault.id(), "claim_vault")
        .deposit(NearToken::from_yoctonear(storage_cost.0))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    // Fetch vault state and check ownership + listing status
    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert_eq!(
        state.owner,
        new_vault_owner.id().as_str(),
        "Vault owner should be new_vault_owner"
    );
    assert!(
        !state.is_listed_for_takeover,
        "Vault should no longer be listed for takeover"
    );

    // Confirm vault_claimed event was logged
    let logs = outcome.logs();
    let matched = logs
        .iter()
        .any(|log| log.contains("EVENT_JSON") && log.contains(r#""event":"vault_claimed""#));
    assert!(
        matched,
        "Expected vault_claimed event log not found. Logs: {:?}",
        logs
    );

    // Confirm old owner received the payment
    let final_root_balance = root.view_account().await?.balance.as_yoctonear();
    let received = final_root_balance - initial_root_balance;
    assert!(
        received >= storage_cost.0,
        "Old owner should receive vault price. Got {} yoctoNEAR",
        received
    );

    Ok(())
}

#[tokio::test]
async fn test_list_and_cancel_takeover_toggles_state() -> anyhow::Result<()> {
    let (_, root, _) = setup_sandbox_and_accounts().await?;
    let vault = test_utils::initialize_test_vault_on_sub_account(&root)
        .await?
        .contract;

    // List the vault for takeover and verify the flag flips on
    root.call(vault.id(), "list_for_takeover")
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        state.is_listed_for_takeover,
        "Vault should be listed after call"
    );

    // Cancel the takeover and expect the listing flag to reset
    root.call(vault.id(), "cancel_takeover")
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let state: VaultViewState = vault.view("get_vault_state").await?.json()?;
    assert!(
        !state.is_listed_for_takeover,
        "Vault should no longer be listed after cancellation"
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_vault_rejects_when_not_listed() -> anyhow::Result<()> {
    let (_, root, buyer) = setup_sandbox_and_accounts().await?;
    let vault = test_utils::initialize_test_vault_on_sub_account(&root)
        .await?
        .contract;

    let storage_cost: U128 = vault.view("view_storage_cost").await?.json()?;

    let outcome = buyer
        .call(vault.id(), "claim_vault")
        .deposit(NearToken::from_yoctonear(storage_cost.0))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    assert!(
        outcome.is_failure(),
        "Claim should fail when vault is not listed"
    );
    let err = outcome.into_result().unwrap_err().to_string();
    assert!(
        err.contains("Vault is not listed for takeover"),
        "Unexpected error message: {err}"
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_vault_rejects_wrong_deposit() -> anyhow::Result<()> {
    let (_, root, buyer) = setup_sandbox_and_accounts().await?;
    let vault = test_utils::initialize_test_vault_on_sub_account(&root)
        .await?
        .contract;

    let storage_cost: U128 = vault.view("view_storage_cost").await?.json()?;

    root.call(vault.id(), "list_for_takeover")
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let wrong_deposit = NearToken::from_yoctonear(storage_cost.0 + 1);
    let outcome = buyer
        .call(vault.id(), "claim_vault")
        .deposit(wrong_deposit)
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    assert!(
        outcome.is_failure(),
        "Claim should fail with mismatched deposit"
    );
    let err = outcome.into_result().unwrap_err().to_string();
    assert!(
        err.contains("Must attach exactly"),
        "Unexpected error message: {err}"
    );

    Ok(())
}

#[tokio::test]
async fn test_claim_vault_rejects_self_claim() -> anyhow::Result<()> {
    let (_, root, _) = setup_sandbox_and_accounts().await?;
    let vault = test_utils::initialize_test_vault_on_sub_account(&root)
        .await?
        .contract;

    let storage_cost: U128 = vault.view("view_storage_cost").await?.json()?;

    root.call(vault.id(), "list_for_takeover")
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let outcome = root
        .call(vault.id(), "claim_vault")
        .deposit(NearToken::from_yoctonear(storage_cost.0))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?;

    assert!(
        outcome.is_failure(),
        "Owner should not be able to claim vault"
    );
    let err = outcome.into_result().unwrap_err().to_string();
    assert!(
        err.contains("Current vault owner cannot claim their own vault"),
        "Unexpected error message: {err}"
    );

    Ok(())
}
