#![cfg(feature = "integration-test")]

use near_sdk::NearToken;

#[path = "test_utils.rs"]
mod test_utils;

use test_utils::{setup_sandbox_and_accounts, VaultViewState};

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
