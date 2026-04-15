# Contract Testing Pipeline

## Overview

The `ContractTestingPipeline` provides a unified, modular workflow for testing WASM contracts. It orchestrates binary checking, genesis setup, and contract deployment.

**Location:** `bin/darkfid/src/tests/pipeline.rs`

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  ContractTestingPipeline                      │
├─────────────────────────────────────────────────────────────┤
│  BinaryChecker                                              │
│    ├─► check_wasm() → BinaryStatus                         │
│    └─► check_zkbins() → Vec<BinaryInfo>                    │
│                                                              │
│  GenesisRunner                                              │
│    ├─► check_genesis() → GenesisStatus                      │
│    └─► ensure_genesis() → GenesisState (stores harness)     │
│                                                              │
│  ContractDeployer                                           │
│    └─► deploy() → ContractId                                │
└─────────────────────────────────────────────────────────────┘
```

## Contract Types

### Native Contracts (Mandatory)

Deployed at genesis - NOT optional:

| Contract | Purpose |
|----------|---------|
| Native Token | Block rewards, fees, burns |
| Deployooor | WASM contract deployment |

### User Contracts (Optional)

Deployed via Deployooor AFTER genesis:

- DEX, Stablecoin, Identity, etc.

## Binary Status Types

```rust
pub enum BinaryStatus {
    Missing,  // Binary doesn't exist
    Stale,    // Binary exists but source is newer
    Current,  // Binary exists and is up-to-date
}
```

## Genesis Status Types

```rust
pub enum GenesisStatus {
    NotStarted,                    // No genesis state
    Ready(GenesisState),          // Genesis complete
    Invalid,                      // Corrupted state
}
```

## API Reference

### `ContractTestingPipeline::new(contract_name, config, ex)`

Create a new pipeline:

```rust
let mut pipeline = ContractTestingPipeline::new("dex", config, &ex).await?;
```

### `pipeline.check_wasm() -> BinaryStatus`

Check if WASM binary exists and is current.

### `pipeline.check_zkbins() -> Vec<BinaryInfo>`

Check ZK binary status.

### `pipeline.check_genesis() -> Result<GenesisStatus>`

Check genesis state.

### `pipeline.build_if_needed() -> Result<BinaryStatus>`

Build WASM if missing or stale.

### `pipeline.ensure_genesis() -> Result<GenesisState>`

Run genesis if needed (stores `GenesisHarness` internally).

### `pipeline.deploy() -> Result<ContractId>`

Deploy contract using stored genesis harness.

### `pipeline.ensure_ready_and_deploy() -> Result<ContractId>`

One-shot: build + genesis + deploy.

### `pipeline.status_report() -> Result<PipelineStatusReport>`

Get complete status of all components.

## Usage Examples

### One-shot (most common)

```rust
let contract_id = ContractTestingPipeline::new("dex", config, &ex)
    .await
    .ensure_ready_and_deploy()
    .await?;
```

### Step-by-step (for debugging)

```rust
let mut pipeline = ContractTestingPipeline::new("dex", config, &ex).await?;

let report = pipeline.status_report().await;
info!("WASM: {:?}", report.wasm_status);
info!("ZK bins: {} found", report.zkbin_status.len());
info!("Genesis: {:?}", report.genesis_status);

if matches!(report.wasm_status, BinaryStatus::Missing) {
    pipeline.build_if_needed().await?;
}

pipeline.ensure_genesis().await?;
let contract_id = pipeline.deploy().await?;
```

## Relationship to GenesisHarness

`ContractTestingPipeline` uses `GenesisHarness` internally for genesis setup:

- `GenesisHarness` = NativeToken + Deployooor baseline only
- `ContractTestingPipeline` = GenesisHarness + binary checking + deployment

You can also use `GenesisHarness` directly if you only need the baseline:

```rust
let mut genesis = GenesisHarness::new(config, &ex).await?;
genesis.generate_genesis_blocks(3).await?;
```

## File Locations

| File | Purpose |
|------|---------|
| `bin/darkfid/src/tests/pipeline.rs` | ContractTestingPipeline |
| `bin/darkfid/src/tests/genesis.rs` | GenesisHarness |
| `bin/darkfid/src/tests/deployer.rs` | ContractDeployer |

## See Also

- [Genesis Harness](./genesis_harness.md) - NativeToken + Deployooor baseline only
- [Test Harness Guide](./test_harness_guide.md) - Full testing overview
- [Sync Module](./sync.md) - Stateless block verification
