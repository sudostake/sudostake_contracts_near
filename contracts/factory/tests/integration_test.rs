use near_workspaces::{sandbox, types::NearToken};
use serde_json::json;

#[tokio::test]
async fn test_factory_deployment() -> Result<(), Box<dyn std::error::Error>> {
    let worker = sandbox().await?;

    // Create necessary test accounts
    let root = worker.root_account()?;
    let sudostake = root
        .create_subaccount("sudostake")
        .initial_balance(NearToken::from_yoctonear(
            100_000_000_000_000_000_000_000_000,
        ))
        .transact()
        .await?
        .into_result()?;

    // Deploy factory contract to sudostake
    let factory_wasm = std::fs::read("../../res/factory.wasm")?;
    let factory_exec = sudostake.deploy(&factory_wasm).await?;
    let factory_contract = factory_exec.into_result()?;
    println!("Factory contract deployed at: {:?}", factory_contract.id());

    // Initialize the factory contract
    let res = factory_contract
        .call("new")
        .args_json(json!({ "owner": sudostake.id() }))
        .transact()
        .await?;
    println!("Factory contract initialization result: {:?}", res);
    assert!(res.is_success(), "Factory contract initialization failed: {:?}", res);

    // Fetch and print vault versions
    let vault_versions: Vec<u32> = factory_contract.view("get_vault_versions").await?.json()?;
    println!("Vault versions: {:?}", vault_versions);

    assert_eq!(
        vault_versions.len(),
        0,
        "Vault versions should initially be empty"
    );

    Ok(())
}
