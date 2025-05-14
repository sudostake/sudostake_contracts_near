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
chmod +x build.sh factory_test.sh vault_test.sh
./build.sh && ./factory_test.sh && ./vault_test.sh
```

## Build SudoStake AI agent
```
python3 -m venv .venv
source .venv/bin/activate
pip install semver
brew install jq  # macOS  (or: sudo apt install jq on Debian/Ubuntu)

chmod +x ./agent/build.sh
source ~/.zshrc && ./agent/build.sh patch && sudo -E nearai agent interactive --local
```

## Run the agent locally in interractive mode
```
nearai login
nearai agent interactive --local
```