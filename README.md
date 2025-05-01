# SudoStake NEAR

## Build all contracts
```
chmod +x build.sh
./build.sh
```

## Test all contracts
```
# Standard test
chmod +x build.sh factory_test.sh vault_test.sh
./build.sh && ./factory_test.sh && ./vault_test.sh
```

## Build SudoStake AI agent
```
source .venv/bin/activate
pip install semver
brew install jq  # macOS  (or: sudo apt install jq on Debian/Ubuntu)

chmod +x ./agent/build.sh
./agent/build.sh patch
```

## Run the agent locally in interractive mode
```
nearai login
nearai agent interactive --local
```