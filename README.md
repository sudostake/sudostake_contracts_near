# SudoStake NEAR

## Build all contracts
```
chmod +x build.sh
./build.sh
```

&nbsp;

## Build Artifacts

| Name     | Description    | Repo     |
|----------|----------------|----------|
| factory.wasm | Proxy for minting vaults | [factory](contracts/factory) |
| vault.wasm | Staking with peer-to-peer options trading | [vault](contracts/vault) |
| staking_pool.wasm  | Official NEAR staking/delegation contract  | [staking-pool](https://github.com/near/core-contracts/tree/master/staking-pool) |
| fungible_token.wasm  | NEP-141 token contract  | [canonical FT contract](https://github.com/near-examples/FT) |

&nbsp;

## Test all contracts
```
# Standard test
chmod +x factory_test.sh vault_test.sh
./factory_test.sh && ./vault_test.sh
```
