# DarkFi Contract Status

**Last Updated**: 2026-04-15

## Contract Architecture

DarkFi has two types of contracts:

### Native Contracts (Mandatory - Deployed at Genesis)

| Contract | Package | Purpose |
|----------|---------|---------|
| Native Token | `darkfi_native_token_contract` | Consensus-first token for block rewards, fees, burns |
| Deployooor | `darkfi_deployooor_contract` | WASM contract deployment |

### User Contracts (Optional - Deployed via Deployooor After Genesis)

All other contracts (DEX, Stablecoin, Identity, etc.) are **user contracts** deployed via Deployooor after genesis. They are not mandatory for blockchain operation.

## Contract Testing Architecture

### Testing Modules

| Module | Location | Purpose |
|--------|----------|---------|
| `GenesisHarness` | `bin/darkfid/src/tests/genesis.rs` | NativeToken + Deployooor baseline only |
| `ContractDeployer` | `bin/darkfid/src/tests/deployer.rs` | Orchestrates genesis + deployment |
| `ContractTestingPipeline` | `bin/darkfid/src/tests/pipeline.rs` | Unified: binary check + genesis + deploy |

### Contract Status

For current contract integration test status, see the test harness in `bin/darkfid/src/tests/`.

## See Also

- [Genesis Harness](./doc/src/arch/genesis_harness.md) - NativeToken + Deployooor baseline
- [Contract Testing Pipeline](./doc/src/arch/pipeline.md) - Unified testing workflow
- [Test Harness Guide](./doc/src/arch/test_harness_guide.md) - Testing overview
