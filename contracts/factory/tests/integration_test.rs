use near_workspaces::{sandbox, Account, Contract, Worker};

#[tokio::test]
async fn test_factory_deployment() -> Result<(), Box<dyn std::error::Error>> {
    let worker = sandbox().await?;
    assert!(true);  // Example assertion
    Ok(())
}