# SudoStake Factory Contract (NEAR)

This contract powers the creation and management of user-owned staking vaults on the NEAR blockchain. Each vault is deployed as a subaccount and initialized with the included vault wasm bytes as an immutable smart contract.

Part of the [SudoStake Protocol](https://sudostake.com).

&nbsp;

## Features

- Set and enforce vault creation fees
- Mint new immutable vault contracts
- Withdraw factory contract NEAR safely
- Transfer ownership of the factory
- Expose view methods for external tooling

&nbsp;

## Contract Methods
&nbsp;

### `new(owner: AccountId, vault_minting_fee: NearToken) -> Self`

**Description**  
Initializes the factory contract with an owner and an initial vault creation fee.

**Access Control**  
Callable only once, immediately after contract deployment.

**Event JSON emitted**  
_None_

&nbsp;


### `set_vault_creation_fee(new_fee: NearToken)`

**Description**  
Sets the required fee (in yoctoNEAR) for vault creation.

**Access Control**  
Only callable by the factory `owner`.

**Event JSON emitted**
```json
{
  "event": "vault_creation_fee_updated",
  "data": {
    "new_fee": "1000000000000000000000000"
  }
}
```

&nbsp;

### `mint_vault() -> Promise`  
(**payable**)

**Description**  
Creates a new vault smart contract as a subaccount (e.g., `vault-0.factory.near`). Vault is initialized with `owner`, `index`, and `version`.

**Access Control**  
Public — any user can call this with exact deposit = `vault_minting_fee`.

**Event JSON emitted**
```json
{
  "event": "vault_minted",
  "data": {
    "owner": "caller.near",
    "vault": "vault-0.factory.near",
    "version": 1,
    "index": 0
  }
}
```

&nbsp;

### `withdraw_balance(amount: NearToken, to_address: Option<AccountId>) -> Promise`

**Description**  
Withdraws available balance (excluding storage reserve) to a recipient.

**Access Control**  
Only callable by the factory `owner`.

**Event JSON emitted**  
_None_

&nbsp;

### `transfer_ownership(new_owner: AccountId)`

**Description**  
Transfers ownership of the contract to a new account.

**Access Control**  
Only callable by the current factory `owner`.

**Event JSON emitted**
```json
{
  "event": "ownership_transferred",
  "data": {
    "old_owner": "old.near",
    "new_owner": "new.near"
  }
}
```

&nbsp;

&nbsp;

## View Methods

### `get_contract_state() -> FactoryViewState`

**Description**  
Returns the current state of the contract:

```json
{
  "owner": "factory_owner.near",
  "vault_minting_fee": "1000000000000000000000000",
  "vault_counter": 3,
}
```

**Access Control**  
Public

&nbsp;

### `storage_byte_cost() -> NearToken`

**Description**  
Returns the protocol-defined storage cost per byte.

**Access Control**  
Public

&nbsp;

## Storage and Deployment Notes

- Minting fee must cover:
  - `WASM size × env::storage_byte_cost()`
  - Plus a storage buffer of 0.01 NEAR
- Vault subaccounts are named: `vault-<index>.factory.near`

