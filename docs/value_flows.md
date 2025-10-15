# SudoStake (NEAR) — Value Flows

A short, sectioned guide showing how funds move through the Vault. Each section has a diagram and just enough rules to integrate safely. Diagrams are Mermaid and render on GitHub.

Diagram conventions
Legend: 🔵 Users, 🟢 Protocol; ┄ dashed = user↔protocol; — solid = protocol↔protocol.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  U[User]
  V[Vault]
  P[Validator Pool]
  U -.-> V
  V --> P
  classDef user fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  class U user
  class V,P protocol
```

## Glossary
- Factory: Contract that mints per‑user Vaults (as subaccounts) and charges a minting fee.
- Vault: User‑owned contract/account that holds NEAR/FTs, stakes, requests liquidity, repays, and may be liquidated.
- FT: NEP‑141 fungible token (e.g., USDC equivalents on NEAR).
- Validator Pool: Standard NEAR staking pool contract.
- yoctoNEAR: 10^-24 NEAR, the minimum attached deposit required for many permissioned calls.

## Table of contents
- 1. Create a vault
- 2. Fund your vault (NEAR in)
- 3. Staking lifecycle
- 4. Liquidity: request, offers, accept
- 5. Repay
- 6. Liquidation (after expiry, NEAR out)
- 7. Refunds and retries
- 8. Ownership and takeover
- 9. Safety mechanisms (built‑ins)
- Appendix A: Staking event JSON examples



## 1. Create a vault
- Call `mint_vault` on the Factory with exactly the mint fee attached.
- Factory creates `vault-<index>.<factory_id>`, deploys `vault.wasm`, initializes it.
- Any leftover NEAR remains as Factory revenue.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  subgraph Users
    O[Owner]
  end
  subgraph Protocol
    F[Factory]
    V[Vault]
  end
 O -.-> F
 F --> V
 classDef user fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
 classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
 class O user
 class F,V protocol
```



## 2. Fund your vault (NEAR in)
- Send NEAR directly to the Vault account name.
- Balance increases immediately; no method call needed.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  O[Owner]
  V[Vault]
  O -.->|NEAR| V
  classDef user fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  class O user
  class V protocol
```



## 3. Staking lifecycle

- Overview
  - Up to `MAX_ACTIVE_VALIDATORS` active validators (currently 2).
  - Long operations run under a processing lock to avoid overlaps.

- Delegate (stake)
  - Call: `delegate(validator, amount)`, attach 1 yoctoNEAR.
  - Preconditions:
    - Owner only; `amount > 0`; `amount <= available` (respects storage buffer).
    - No pending refunds; no liquidation in progress.
    - New validator does not exceed `MAX_ACTIVE_VALIDATORS`.
  - Action: `deposit_and_stake(amount)` on the validator pool.
  - Emits: `delegate_completed` on success; `delegate_failed` on failure.
  - State: Validator added to `active_validators` on success.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  O[Owner]
  V[Vault]
  P[Validator Pool]
  O -.-> V
  V -->|deposit_and_stake| P
  classDef user fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  class O user
  class V,P protocol
```

- Undelegate (start unlock)
  - Call: `undelegate(validator, amount)`, attach 1 yoctoNEAR.
  - Preconditions:
    - Owner only; `amount > 0`.
    - `validator` is in `active_validators`.
    - No open liquidity request.
  - Action: `unstake(amount)`, then view remaining staked balance.
  - Emits: `undelegate_completed` on success; `undelegate_failed` on failure; `validator_removed` if balance becomes zero.
  - State: Records `UnstakeEntry { amount, epoch_height }` for the validator.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  O[Owner]
  V[Vault]
  P[Validator Pool]
  O -.-> V
  V -->|unstake| P
  classDef user fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  class O user
  class V,P protocol
```

- Claim (withdraw unlocked)
  - Call: `claim_unstaked(validator)`, attach 1 yoctoNEAR.
  - Preconditions:
    - `UnstakeEntry` exists and `epoch >= entry.epoch_height + NUM_EPOCHS_TO_UNLOCK` (4).
    - No liquidation in progress.
  - Action: `withdraw_all()` on the validator pool.
  - Emits: `claim_unstaked_completed` on success; `claim_unstake_failed` on failure.
  - State: Clears `UnstakeEntry`; liquid NEAR returns to the Vault balance.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  V[Vault]
  P[Validator Pool]
  V -->|withdraw_all| P
  P -->|NEAR unlocked| V
  classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  class V,P protocol
```

- Notes
  - Rewards accrue while staked and become withdrawable after unlock + withdraw.
  - Delegation is blocked when refunds are pending or liquidation is active.

- Example timeline (epochs)
  - E100: Owner calls `delegate(pool.stakehouse.near, 50 NEAR)` — funds become staked.
  - E102: Owner calls `undelegate(pool.stakehouse.near, 10 NEAR)` — records `UnstakeEntry { amount: 10 NEAR, epoch_height: 102 }`.
  - E106: Earliest epoch when `claim_unstaked` is allowed (102 + NUM_EPOCHS_TO_UNLOCK = 4).
  - E106+: Owner calls `claim_unstaked(pool.stakehouse.near)` — Vault executes `withdraw_all()`, liquid NEAR returns to the Vault’s balance.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  subgraph Users
    O[Owner]
  end
  subgraph Protocol
    V[Vault]
    P[Validator Pool]
  end
  O -.-> V
  V -.-> O
  V --> P
  P --> V
  classDef user fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  class O user
  class V,P protocol
```



## 4. Liquidity: request, offers, accept
- `request_liquidity`: Owner posts request (token, amount, interest, collateral, duration).
- Lenders send `ft_transfer_call` with `ApplyCounterOffer` (matching the requested amount accepts immediately; lower amounts become counter offers if eligible).
- Owner may accept a single counter offer; non‑winners are refunded.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  subgraph Users
    O[Owner]
    L[Lender]
  end
  subgraph Protocol
    V[Vault]
    T[FT Token]
  end
  O -.-> V
  V -.-> O
  L -.-> T
  T --> V
  classDef user fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  class O,L user
  class V,T protocol
```



## 5. Repay
- `repay_loan`: Owner triggers repayment of principal + interest in the requested token.
- Vault calls `ft_transfer` to pay the lender. On success, loan state clears.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  subgraph Protocol
    V[Vault]
    T[FT Token]
  end
  subgraph Users
    L[Lender]
  end
  V --> T
  T -.-> L
  classDef user fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  class L user
  class V,T protocol
```



## 6. Liquidation (after expiry, NEAR out)
- Anyone can call `process_claims` after expiry of an accepted request.
- Pay order: liquid NEAR → matured unstaked → new unstaking (callbacks, lock‑guarded).
- State clears when total due is paid.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  subgraph Protocol
    V[Vault]
    P[Validator Pool]
  end
  subgraph Users
    L[Lender]
  end
  V <--> P
  V -.->|NEAR| L
  classDef user fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  class L user
  class V,P protocol
```



## 7. Refunds and retries
- Refunds use `ft_transfer` (for tokens) or NEAR transfer; failures are saved to `refund_list`.
- Anyone entitled (owner or proposer) can retry refunds by ID; success removes the entry.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  subgraph Protocol
    V[Vault]
    T[FT Token]
  end
  subgraph Users
    PZ[Proposer]
  end
  V --> T
  T -.-> PZ
  V -.-> PZ
  classDef user fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  class PZ user
  class V,T protocol
```



## 8. Ownership and takeover
- `transfer_ownership`: Owner changes the owner field (no value transfer).
- `claim_vault`: Anyone can claim a listed vault by attaching exactly the storage cost.
  - Vault forwards that NEAR to the old owner; on success, ownership updates.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart LR
  subgraph Users
    C[Claimant]
    O[Old Owner]
  end
  subgraph Protocol
    V[Vault]
  end
  C -.->|NEAR storage cost| V
  V -.->|NEAR| O
  classDef user fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef protocol fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  class C,O user
  class V protocol
```



## 9. Safety mechanisms (built‑ins)
- Storage buffer: Keeps a small NEAR reserve so storage costs never delete state.
- Processing lock: Long actions run one at a time with a timeout to avoid overlaps.
- Deterministic ordering: Validators are sorted to avoid index races.

```mermaid
%%{init: {'flowchart': {'curve': 'step'}}}%%
flowchart TB
  V[Vault]
  subgraph Guards
    Lk[Processing Lock]
    Rf[Refund Ledger]
    Sb[Storage Buffer]
  end
  V --> Lk
  V --> Rf
  V --> Sb
```



## For integrators
- Index `EVENT_JSON` logs (e.g., `vault_created`, `liquidity_request_opened`, `counter_offer_created`,
  `liquidity_request_accepted`, `repay_loan_successful`, `liquidation_started`, `liquidation_complete`,
  `withdraw_near`, `withdraw_ft`, `refund_failed`, `retry_refund_succeeded`, `lock_acquired`/`lock_released`).
- Respect the processing lock: retry after release or timeout.
- Budget for storage on Factory and Vault calls.



## Appendix A: Staking event JSON examples
- All events are emitted as log lines prefixed with `EVENT_JSON:`. Amounts are stringified yoctoNEAR.

- Delegate — success
```
EVENT_JSON:{
  "event": "delegate_completed",
  "data": {
    "vault": "vault-0.factory.near",
    "validator": "pool.stakehouse.near",
    "amount": "50000000000000000000000000"
  }
}
```

- Delegate — failure
```
EVENT_JSON:{
  "event": "delegate_failed",
  "data": {
    "vault": "vault-0.factory.near",
    "validator": "pool.stakehouse.near",
    "amount": "50000000000000000000000000",
    "error": "deposit_and_stake failed"
  }
}
```

- Undelegate — success
```
EVENT_JSON:{
  "event": "undelegate_completed",
  "data": {
    "vault": "vault-0.factory.near",
    "validator": "pool.stakehouse.near",
    "amount": "10000000000000000000000000"
  }
}
```

- Undelegate — failure
```
EVENT_JSON:{
  "event": "undelegate_failed",
  "data": {
    "vault": "vault-0.factory.near",
    "validator": "pool.stakehouse.near",
    "amount": "10000000000000000000000000",
    "error": "unstake failed"
  }
}
```

- Validator removed from active set
```
EVENT_JSON:{
  "event": "validator_removed",
  "data": {
    "vault": "vault-0.factory.near",
    "validator": "pool.stakehouse.near"
  }
}
```

- Claim unstaked — success
```
EVENT_JSON:{
  "event": "claim_unstaked_completed",
  "data": {
    "vault": "vault-0.factory.near",
    "validator": "pool.stakehouse.near"
  }
}
```

- Claim unstaked — failure
```
EVENT_JSON:{
  "event": "claim_unstake_failed",
  "data": {
    "vault": "vault-0.factory.near",
    "validator": "pool.stakehouse.near",
    "error": "withdraw_all failed"
  }
}
```
