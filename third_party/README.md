# Third-party Wasm Assets

This directory stores pre-built Wasm modules that originate outside this repository but are required for integration tests:

- `wasm/staking_pool.wasm` — official staking pool contract from `near/core-contracts`
- `wasm/fungible_token.wasm` — NEP-141 reference implementation from `near-examples/FT`

`./scripts/prepare_res_dirs.sh` copies these binaries into `res/` so the test suite can deploy them alongside the locally built contracts. When upstream releases new versions you would like to test against, replace the binaries here and re-run the helper scripts.
