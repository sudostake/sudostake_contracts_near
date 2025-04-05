### 📦 Factory Contract

A smart contract responsible for deploying versioned, immutable staking vaults as subaccounts on NEAR Protocol. It manages vault WASM uploads, versioning, and fee collection.

---

### 🚀 Features

- **Immutable Vault Deployment**: Mints smart contract subaccounts from pre-uploaded WASM.
- **Vault Versioning**: Tracks vault code via SHA-256 hashes and versions.
- **Dynamic Fee Control**: Factory owner can update minting fees.
- **Secure Access Control**: Only owner can perform sensitive actions.
- **Balance Management**: Owner can withdraw available contract funds.
- **Event Logging**: Emits JSON-formatted logs for all key events.

---

### ⚙️ Usage Overview

#### Initialization
```ts
factory.new({ owner, vault_minting_fee });
```

#### Upload Vault Code
```ts
factory.set_vault_code({ code: Uint8Array }); // Must be >1KB
```

#### Mint Vault
```ts
factory.mint_vault(); // Must attach exact minting fee
```

#### Withdraw Balance
```ts
factory.withdraw_balance({ amount, to_address }); // `to_address` optional
```

#### Transfer Ownership
```ts
factory.transfer_ownership({ new_owner });
```

---

### 📡 View Methods

- `get_contract_state()` → `{ vault_minting_fee, vault_counter, latest_vault_version }`
- `storage_byte_cost()` → `NearToken` (dynamic per protocol)

---

### 🔐 Access Control

- `owner` can upload vault code, update fees, withdraw balance, and transfer ownership.
- `mint_vault()` is public but enforces exact fee and valid vault code.

---

### 📁 Contract Structure

- `factory.wasm`: Deployable WASM contract
- `vault.wasm`: Code uploaded by factory and used for sub-deployments
- `res/`: Optimized WASM outputs

---

### 🧪 Testing

- Fully covered by **unit** and **integration** tests.
- Uses [`near-sdk`](https://crates.io/crates/near-sdk) and [`near-workspaces`](https://github.com/near/near-workspaces-rs)
