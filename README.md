# SudoStake NEAR

## Build contracts independently
```
cd contracts/factory && cargo near build non-reproducible-wasm --locked
cd contracts/vault && cargo near build non-reproducible-wasm --locked
```

## Build all contract
```
chmod +x build.sh
./build.sh
```
