# SudoStake NEAR

## Build all contracts
```
chmod +x build.sh
./build.sh
```

## Test all contracts
```
# Standard test
cargo test --release

# To see logs in console
cargo test -- --nocapture
```