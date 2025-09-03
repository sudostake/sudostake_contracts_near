# SudoStake NEAR

Monorepo with NEAR smart contracts and docs for the SudoStake protocol.

Repository layout
- contracts/factory — mints per‑user Vaults (immutable subaccounts)
- contracts/vault — staking + peer‑to‑peer liquidity/loans logic
- res/ — compiled .wasm artifacts
- build.sh, factory_test.sh, vault_test.sh — build and test helpers


## Prerequisites

Before building, make sure you have:

- Rust toolchain (stable) and the wasm32 target:
  - rustup target add wasm32-unknown-unknown
- Binaryen (provides wasm-opt):
  - macOS: brew install binaryen
  - Debian/Ubuntu: sudo apt install binaryen


## Build all contracts
```
chmod +x build.sh
./build.sh
```

Outputs are written to ./res as factory.wasm and vault.wasm and are size‑optimized via wasm-opt.


## Build Artifacts

| Name | Description | Repo |
|------|-------------|------|
| factory.wasm | Proxy for minting vaults | [factory](contracts/factory) |
| vault.wasm | Staking with peer‑to‑peer options trading | [vault](contracts/vault) |
| staking_pool.wasm | Official NEAR staking/delegation contract | [staking-pool](https://github.com/near/core-contracts/tree/master/staking-pool) |
| fungible_token.wasm | NEP‑141 token contract | [canonical FT contract](https://github.com/near-examples/FT) |


## Contracts and key methods

- Factory
  - Docs: contracts/factory/README.md
  - Methods (high level):
    - new(owner, vault_minting_fee)
    - set_vault_creation_fee(new_fee)
    - mint_vault() [payable]
    - withdraw_balance(amount, to?)
    - transfer_ownership(new_owner)
    - Views: get_contract_state(), storage_byte_cost()

- Vault
  - Code: contracts/vault/
  - Methods (high level):
    - Staking: delegate(validator, amount) [payable 1y], undelegate(validator, amount) [payable 1y], claim_unstaked(validator) [payable 1y]
    - Liquidity: request_liquidity(token, amount, interest, collateral, duration) [payable 1y], try_add_counter_offer(msg via ft_transfer_call), try_accept_liquidity_request(msg via ft_transfer_call), cancel_counter_offer(), cancel_liquidity_request()
    - Repayment/Liquidation: repay_loan() [payable 1y], process_claims()
    - Ownership & Withdrawals: withdraw_balance(token?, amount, to?) [NEAR or NEP‑141], transfer_ownership(new_owner), claim_vault() [payable], retry_refunds(ids)
    - Views: get_vault_state(), get_active_validators(), get_unstake_entry(validator), view_available_balance(), view_storage_cost(), get_refund_entries(owner?)


## Quickstart: local end‑to‑end with near‑workspaces

This repo includes near‑workspaces integration tests that spin up a NEAR sandbox, deploy contracts, and exercise flows.

1) Build test artifacts
```
# Builds a vault.wasm with the `integration-test` feature at ./vault_res/vault.wasm
./vault_test.sh
```

2) Run all vault tests (near‑workspaces sandbox)
```
cargo test -p vault --release --features integration-test
```

3) Run a focused test
```
cargo test -p vault --release --features integration-test delegate_tests
```

4) Minimal near‑workspaces snippet (for your own tests)
```
// tests/my_sandbox_test.rs
use near_workspaces::{sandbox, Account, Contract};
use near_sdk::NearToken;

#[tokio::test]
async fn deploy_and_init_vault() -> anyhow::Result<()> {
    // Start sandbox and get root account
    let worker = sandbox().await?;
    let root: Account = worker.root_account()?;

    // Deploy compiled wasm (built by ./vault_test.sh)
    let wasm_bytes = std::fs::read("vault_res/vault.wasm")?;
    let vault: Contract = root.deploy(&wasm_bytes).await?.into_result()?;

    // Initialize vault
    vault
        .call("new")
        .args_json(serde_json::json!({
            "owner": root.id(),
            "index": 0,
            "version": 1
        }))
        .transact()
        .await?
        .into_result()?;

    // Example: view state
    let state: serde_json::Value = vault.view("get_vault_state").await?.json()?;
    println!("state: {}", state);
    Ok(())
}
```


## Test all contracts
```
# Runs unit/integration tests. Requires native Rust target.
# The vault tests also rebuild a wasm with the `integration-test` feature.
chmod +x factory_test.sh vault_test.sh
./factory_test.sh && ./vault_test.sh
```


## Value flows (how funds move)

For a plain‑language overview of how NEAR and tokens move through the system, see:

- value_flows.md (renders Mermaid diagrams on GitHub)
