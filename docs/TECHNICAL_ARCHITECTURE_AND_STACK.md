Title: SudoStake (NEAR) — Technical Architecture & Stack

Overview
- This repository contains the on-chain smart contracts and documentation for the SudoStake protocol on NEAR.
- Two contracts are shipped and versioned together:
  - Factory: deploys per-user Vault contracts as subaccounts and charges a minting fee.
  - Vault: user-owned contract that holds NEAR and tokens, participates in NEAR staking, requests liquidity, accepts/handles offers, supports repayment, and automates liquidation after loan expiry.


Runtime and Tooling Stack
- Language: Rust (Edition 2021)
- Target: wasm32-unknown-unknown
- NEAR SDK: near-sdk = 5.11.0; near-contract-standards = 5.11.0
- Serialization: borsh 1.2.0 for state; serde 1.0 for JSON I/O and events
- Testing:
  - Unit tests: native Rust tests using near-sdk unit-testing helpers
  - Integration tests: near-workspaces = 0.19.0 (sandbox); gating via feature = "integration-test"
- Build tooling:
  - Binaryen (wasm-opt) for size optimization (required by build.sh)
  - Scripts: build.sh, factory_test.sh, vault_test.sh
  - Reproducible build metadata (cargo-near container) is recorded in crate metadata


Contracts
1) Factory (contracts/factory)
   Purpose
   - Mints new Vault subaccounts and deploys vault.wasm code.
   - Holds configurable minting fee and allows owner-controlled withdrawals and ownership transfer.

   Key behavior
   - Includes the vault binary at compile time: include_bytes!("../../../res/vault.wasm").
   - mint_vault (payable):
     - Requires attached deposit == configured vault_minting_fee.
     - Computes cost = vault wasm byte length × env::storage_byte_cost() + STORAGE_BUFFER.
     - Asserts fee covers deployment; creates subaccount vault-{index}.{factory}, transfers NEAR for storage, deploys code, and initializes via new(owner, index, version).
     - Increments internal vault_counter; logs EVENT_JSON vault_minted.
   - set_vault_creation_fee: owner-only, updates fee; logs EVENT_JSON vault_creation_fee_updated.
   - withdraw_balance: owner-only; transfers any balance above storage reserve to a recipient.
   - transfer_ownership: owner-only; updates owner and logs ownership_transferred.
   - Views: get_contract_state() (owner, vault_minting_fee, vault_counter); storage_byte_cost().

   Storage & gas
   - STORAGE_BUFFER = 0.01 NEAR is reserved to avoid state deletion due to storage costs.
   - GAS_FOR_VAULT_INIT = 100 Tgas used for the vault new(...) call.


2) Vault (contracts/vault)
   Purpose
   - Custodies NEAR and NEP-141 tokens on behalf of the owner.
   - Delegates and undelegates NEAR to NEAR staking pools.
   - Allows the owner to request liquidity from lenders against staked NEAR.
   - Handles acceptance of offers, repayment, and forced liquidation after expiry.

   On-chain state (high level)
   - owner: AccountId — vault owner (controls privileged actions)
   - index: u64 — index assigned by factory at creation
   - version: u64 — code version at deployment
   - active_validators: Set<AccountId> — validators with delegated stake
   - unstake_entries: Map<AccountId, UnstakeEntry { amount, epoch_height }>
   - pending_liquidity_request: Option<PendingLiquidityRequest>
   - liquidity_request: Option<LiquidityRequest { token, amount, interest, collateral, duration, created_at }>
   - counter_offers: Option<Map<AccountId, CounterOffer { proposer, amount, timestamp }>> (capacity capped)
   - accepted_offer: Option<AcceptedOffer { lender, accepted_at }>
   - liquidation: Option<Liquidation { liquidated: NearToken }>
   - refund_list: Map<u64, RefundEntry { token?, proposer, amount, added_at_epoch }>
   - refund_nonce: u64 — sequential id for refund entries
   - processing_state: ProcessingState — global lock for long-running flows
   - processing_since: u64 — when the lock was taken (block timestamp in ns)
   - is_listed_for_takeover: bool — enable buyout via claim_vault

   Constants and limits
   - STORAGE_BUFFER = 0.01 NEAR reserved on account
   - NUM_EPOCHS_TO_UNLOCK = 4 epochs before unstaked balance becomes withdrawable
   - REFUND_EXPIRY_EPOCHS = 4 epochs
   - MAX_COUNTER_OFFERS = 7 concurrent offers
   - MAX_ACTIVE_VALIDATORS = 2
   - LOCK_TIMEOUT = 30 minutes
   - Gas budgets (per call type):
     - deposit_and_stake: 120 Tgas; unstake: 60 Tgas; withdraw_all: 35 Tgas each; view calls: 10 Tgas; FT transfers: 30 Tgas; callbacks sized appropriately (see code)

   Processing lock
   - acquire_processing_lock(kind):
     - Ensures only one async workflow is active at a time (Delegate, ClaimUnstaked, RequestLiquidity, Undelegate, RepayLoan, ProcessClaims).
     - Stale locks auto-release after LOCK_TIMEOUT.
     - Logs lock_acquired EVENT_JSON; release logs lock_released.

   Staking flows
   - delegate(validator, amount) [payable 1y]:
     - Owner-only; amount > 0; amount <= available balance (after storage reserve); no pending refunds; no liquidation; validators <= MAX_ACTIVE_VALIDATORS.
     - Calls validator.deposit_and_stake(amount) and, on success, adds validator to active_validators.
   - undelegate(validator, amount) [payable 1y]:
     - Owner-only; validator must be active; amount > 0; no open liquidity request.
     - Calls validator.unstake(amount), then fetches new staked balance; records UnstakeEntry; prunes zero-stake validators.
   - claim_unstaked(validator) [payable 1y]:
     - Owner-only; requires current_epoch >= entry.epoch_height + NUM_EPOCHS_TO_UNLOCK; disallowed during liquidation.
     - Calls validator.withdraw_all(); on success, removes UnstakeEntry; liquid NEAR becomes available in the Vault.

   Liquidity request & offers
   - request_liquidity(token, amount, interest, collateral, duration) [payable 1y]:
     - Owner-only; not already pending/open/accepted; counter_offers must be None.
     - Creates PendingLiquidityRequest, then batch-queries total staked across active validators.
     - If total staked >= collateral, finalizes LiquidityRequest and logs liquidity_request_opened.
  - Offers are submitted via FT transfer hooks (ft_on_transfer):
    - ApplyCounterOffer message: exact match on token, amount, interest, collateral, duration; if the attached amount matches the requested amount, the offer is accepted immediately (clearing counter offers); otherwise, the amount must be > 0 and < requested, and is recorded as a counter offer (enforcing unique proposer, ordering, and MAX_COUNTER_OFFERS eviction/refund rules).
   - accept_counter_offer(proposer, amount) [payable 1y]:
     - Owner-only; request must exist and be unaccepted; amount must match stored offer; sets accepted_offer and refunds all other counter offers.
   - cancel_counter_offer() [payable 1y]: proposer withdraws their own offer; refund attempted.
   - cancel_liquidity_request() [payable 1y]: owner cancels unaccepted request; refunds all counter offers.

   Repayment & liquidation
   - repay_loan() [payable 1y]:
     - Owner-only; requires accepted offer and no liquidation; transfers principal+interest in FT to lender; on success, clears loan state.
   - process_claims() [payable 1y]:
     - Callable by anyone after request expiry (accepted_at + duration).
     - Initializes liquidation if needed; first transfers any liquid NEAR to lender.
     - Then drives a state machine:
       1) If any validators have matured UnstakeEntry entries, batch withdraw_all for them.
       2) Else, if enough is already maturing, wait; otherwise, batch-query staked balances and batch-unstake what’s needed.
     - Clears loan state once total collateral due is transferred to lender.

   Withdrawals & ownership
   - withdraw_balance(token?: Option<AccountId>, amount: U128, to?: Option<AccountId>):
     - For NEAR (token = None): owner-only; amount <= available balance (after storage reserve); additional rules enforced by ensure_owner_can_withdraw.
     - For NEP-141: owner-only; requires 1 yoctoNEAR and calls ft_transfer.
     - Withdrawal rules:
       - If no liquidity request: allowed (NEAR and FTs), subject to storage reserve and refunds being empty.
       - If request pending (no accepted offer): NEAR allowed; FT allowed only if token != requested token.
       - If request accepted and no liquidation: allowed (NEAR and FTs).
       - If liquidation active: NEAR withdrawals blocked; FTs allowed.
   - transfer_ownership(new_owner) [payable 1y]: owner-only; updates owner.
   - list_for_takeover() / cancel_takeover() [payable 1y]: owner toggles takeover flag.
   - claim_vault() [payable]: anyone may claim a listed vault by attaching exactly current storage cost; funds are forwarded to old owner, then ownership is updated.

   Refunds
   - All FT refunds are attempted via ft_transfer with a 1 yoctoNEAR deposit; failures are logged and recorded in refund_list with a nonce id.
   - retry_refunds() [payable 1y]: owner or original proposer can retry their failed refunds; successful retries are removed; failed retries are re-added if not expired (REFUND_EXPIRY_EPOCHS).

   External interfaces used
   - Staking pool (ext_staking_pool): deposit_and_stake(), unstake(amount), withdraw_all(), get_account_staked_balance(account_id) — assumes NEAR core staking-pool standard.
   - Fungible token (ext_fungible_token): ft_transfer(receiver_id, amount, memo?) — NEP-141 standard.

   Events & observability
   - All structured logs are emitted as lines prefixed with "EVENT_JSON:" containing a JSON object: { "event": <string>, "data": <object> }.
   - Examples include: vault_created, liquidity_request_opened, counter_offer_created, counter_offer_evicted, counter_offer_accepted, liquidity_request_accepted, repay_loan_successful, liquidation_started, liquidation_complete, withdraw_near, withdraw_ft, refund_failed, retry_refund_succeeded, lock_acquired, lock_released, etc.


Security & Safety Considerations
- Access control: Most state-changing methods are owner-only; enforcement uses assert_one_yocto() + predecessor checks for intentional calls.
- Storage reserve: A fixed STORAGE_BUFFER plus protocol storage cost is accounted for; available balance excludes this to avoid state deletion.
- Processing lock: Ensures long-running async workflows do not interleave; includes timeout-based stale lock release.
- Deterministic iteration: Active validator sets are sorted before batch calls to avoid non-deterministic ordering/races.
- Limits: MAX_ACTIVE_VALIDATORS, MAX_COUNTER_OFFERS protect against unbounded growth.
- Refund ledger: Failed transfers are persisted and can be retried; retries expire after REFUND_EXPIRY_EPOCHS.


Build & Test
- Build all contracts: ./scripts/build.sh (requires wasm-opt)
  - Copies pinned third-party Wasm into res/, produces res/vault.wasm + res/factory.wasm (wasm-opt -Oz), copies their ABI artifacts (`*_abi.json`, `*_abi.zst`), and records matching `.sha256` hash files for reproducibility checks
- Run unit/integration tests: ./scripts/factory_test.sh && ./scripts/vault_test.sh
  - scripts/factory_test.sh enables the `integration-test` feature so the async sandbox tests are compiled and executed.
  - scripts/vault_test.sh also builds a test-only wasm (feature = integration-test) at vault_res/vault.wasm for near-workspaces sandbox tests.


Repository Structure (selected)
- contracts/factory — factory contract code and tests
- contracts/vault — vault contract code and tests
- res/ — gitignored build outputs used by tests (populated by helper scripts)
- third_party/wasm — pinned Wasm dependencies copied into res/
- docs/value_flows.md — end-to-end value flow diagrams and notes
- README.md — quickstart and reference for key methods


Integration Notes
- Use ft_transfer_call (ft_on_transfer hook) to submit offers to a vault using the ApplyCounterOffer JSON message: { action, token, amount, interest, collateral, duration }.
- Index EVENT_JSON logs for off-chain state tracking.
- Respect the processing lock: retry actions after lock_released or timeout.
- Budget gas appropriately per call; see constants in contracts/vault/src/types.rs and process_claims.rs.


Versioning
- Crate metadata: [package.metadata.near] version = "5.11.0" (SDK compatibility recorded); reproducible build info is included for cargo-near.


Appendix: Key Files
- contracts/factory/src/contract.rs — minting logic and fee handling
- contracts/vault/src/contract.rs — vault state and initialization
- contracts/vault/src/types.rs — constants, types, and view structs
- contracts/vault/src/*.rs — feature modules (staking, liquidity, repayment, liquidation, refunds, ownership)
- contracts/vault/src/ext.rs — external interfaces for staking pools and FT
- contracts/vault/src/macros.rs & contracts/factory/src/macros.rs — EVENT_JSON logging macro
