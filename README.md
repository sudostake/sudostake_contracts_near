# SudoStake NEAR

Monorepo with NEAR smart contracts and docs for the SudoStake protocol.

Repository layout
- contracts/factory — mints per‑user Vaults (immutable subaccounts)
- contracts/vault — staking + peer‑to‑peer liquidity/loans logic
- res/ — compiled .wasm artifacts
- scripts/ — helper tooling (`build.sh`, `factory_test.sh`, `vault_test.sh`, setup utilities)

Further reading
- docs/TECHNICAL_ARCHITECTURE_AND_STACK.md — deep-dive into architecture, stack, and integration


## Step 1: Prepare Environment

Complete these steps in order before building or running tests:

1. **Install Rust stable and the wasm target**
   ```bash
   rustup target add wasm32-unknown-unknown
   ```
2. **Install the Rust 1.86 fallback toolchain**
   ```bash
   rustup toolchain install 1.86.0
   ```
   The current NEAR VM rejects Wasm compiled with Rust 1.87+, so keep 1.86 available for contract builds.
3. **Install cargo-near** (the official NEAR build extension)
   ```bash
   cargo install cargo-near
   ```
4. **Install Binaryen for `wasm-opt`**
   - macOS: `brew install binaryen`
   - Debian/Ubuntu: `sudo apt install binaryen`
5. **Install Docker and start the daemon**
   `cargo near build reproducible-wasm` runs contracts inside the published container image. Start Docker Desktop (or your preferred daemon) before invoking the helper scripts. Apple Silicon users should enable Rosetta/amd64 emulation so the `sourcescan/cargo-near` image can run.
6. **(Linux only) Apply NEAR sandbox kernel parameters**
   ```bash
   sudo scripts/set_kernel_params.sh
   ```
   Run this after rebooting if your distro resets socket limits. macOS already ships with permissive defaults.
7. **(Optional) Cache the NEAR sandbox binary for offline work**
  ```bash
  ./scripts/setup.sh
  export NEAR_SANDBOX_BIN_PATH="$(pwd)/bin/near-sandbox"
  echo "${NEAR_SANDBOX_BIN_PATH:-not set}"
  ```
   This downloads `near-sandbox` into `bin/` and points near-workspaces at it. Add the export to your shell profile to avoid repeated downloads. Set `SANDBOX_VERSION` or `SANDBOX_FORCE=1` when calling `scripts/setup.sh` to choose a different build.

Helper scripts (`scripts/factory_test.sh`, `scripts/vault_test.sh`) honour the optional environment variable `CARGO_NEAR_TOOLCHAIN_OVERRIDE`. Set it to a toolchain that matches the sandbox requirements (e.g. `CARGO_NEAR_TOOLCHAIN_OVERRIDE=1.86.0-aarch64-apple-darwin`) if you need to avoid the newest Rust features when building locally. When the active `rustc` is 1.87 or newer, the scripts automatically look for an installed 1.86 toolchain and fall back to it.


## Step 2: Build Contracts
```
chmod +x scripts/build.sh   # first run only
./scripts/build.sh
```

The script drives `cargo near build reproducible-wasm` for both contracts inside the dockerized toolchain defined in each crate’s `Cargo.toml`. Reproducible builds require a clean git tree; stash or commit any outstanding edits first, or set `CARGO_NEAR_ALLOW_DIRTY=1` when you deliberately want to build from a dirty workspace.

Outputs are written to ./res as factory.wasm and vault.wasm (ABI data is embedded automatically).


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


## Step 3: Run Vault Integration Tests

Follow these steps whenever you want to exercise the full near-workspaces flow with the vault contract.

1. Ensure the sandbox binary is available (skip if you let near-workspaces download it on demand):
   ```bash
   export NEAR_SANDBOX_BIN_PATH="$(pwd)/bin/near-sandbox"
   echo "${NEAR_SANDBOX_BIN_PATH:-not set}"
   ```
   Running `./scripts/setup.sh` during Step&nbsp;1 downloads the default sandbox build into `bin/near-sandbox`. Use `echo "${NEAR_SANDBOX_BIN_PATH:?NEAR_SANDBOX_BIN_PATH is not set}"` if you want the shell to error when the variable is missing.
2. Build the integration-test Wasm and execute the suite:
   ```bash
   chmod +x scripts/vault_test.sh   # first run only
   ./scripts/vault_test.sh
   ```
   The script rebuilds `vault_res/vault.wasm` via `cargo near build non-reproducible-wasm --features integration-test`. If your default toolchain is Rust 1.87+, it automatically falls back to Rust 1.86 (installed in Step&nbsp;1). Because `cargo near` generates ABI metadata by default, ensure any structs returned from view methods derive `schemars::JsonSchema`. Export `RUST_TEST_THREADS=1` if you prefer to run the tests single-threaded.
3. Focus on a single vault test once `vault_res/vault.wasm` is up to date:
   ```bash
   RUST_TEST_THREADS=1 cargo test -p vault --release --features integration-test delegate_tests
   ```
   Rebuild the Wasm first if you modify contract code.


## Step 4: Run Factory Integration Tests

Factory tests follow the same pattern as the vault instructions above:

1. Confirm the environment prep from Step&nbsp;1 (kernel params, Binaryen, optional sandbox binary) is still in place.
2. Rebuild `res/factory.wasm` if you want to run tests manually:
   ```bash
   cargo near build non-reproducible-wasm \
     --manifest-path contracts/factory/Cargo.toml \
     --out-dir res
   ```
   (The helper script below performs this step automatically.)
3. Execute the factory suite:
   ```bash
   chmod +x scripts/factory_test.sh   # first run only
   ./scripts/factory_test.sh   # Automatically rebuilds res/factory.wasm and runs cargo test -p factory
   ```
   The script sets `RUST_TEST_THREADS=1` by default to avoid port conflicts inside the NEAR sandbox.
4. Target specific factory tests after the Wasm is rebuilt:
   ```bash
   RUST_TEST_THREADS=1 cargo test -p factory --release mint_vault_success
   ```

### Run both suites together
```
# Runs unit + integration tests for both contracts (requires the native Rust target).
# Ensure NEAR_SANDBOX_BIN_PATH is exported before invoking these scripts, and
# that res/factory.wasm and vault_res/vault.wasm stay in sync with your latest code (rerun the helper scripts if in doubt).
chmod +x scripts/factory_test.sh scripts/vault_test.sh   # first run only
./scripts/factory_test.sh && ./scripts/vault_test.sh
```


## Value flows (how funds move)

For a plain‑language overview of how NEAR and tokens move through the system, see:

- docs/value_flows.md (renders Mermaid diagrams on GitHub)
