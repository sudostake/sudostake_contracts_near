use near_sdk::NearToken;
use serde_json::json;

use test_utils::{
    get_usdc_balance, initialize_test_token, initialize_test_vault_on_sub_account,
    register_account_with_token, VAULT_CALL_GAS,
};

#[path = "test_utils.rs"]
mod test_utils;

#[tokio::test]
async fn test_ft_on_transfer_refunds_invalid_message() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;

    let vault = initialize_test_vault_on_sub_account(&root).await?.contract;
    let token = initialize_test_token(&root).await?;

    let alice = root
        .create_subaccount("alice")
        .initial_balance(NearToken::from_near(5))
        .transact()
        .await?
        .into_result()?;

    for account in [vault.id(), alice.id()] {
        register_account_with_token(&root, &token, account).await?;
    }

    root.call(token.id(), "ft_transfer")
        .args_json(json!({ "receiver_id": alice.id(), "amount": "1000" }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let initial_balance = get_usdc_balance(&token, alice.id()).await?;

    alice
        .call(token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "100",
            "msg": "not valid json"
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(VAULT_CALL_GAS)
        .transact()
        .await?
        .into_result()?;

    let final_balance = get_usdc_balance(&token, alice.id()).await?;
    assert_eq!(
        final_balance.0, initial_balance.0,
        "Sender balance should be refunded on invalid payload"
    );

    let vault_balance = get_usdc_balance(&token, vault.id()).await?;
    assert_eq!(
        vault_balance.0, 0,
        "Vault should not retain tokens for invalid payload"
    );

    Ok(())
}
