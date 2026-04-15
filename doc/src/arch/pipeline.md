# Contract Testing Pipeline

## Overview

The `ContractTestingPipeline` provides a unified, modular workflow for testing WASM contracts. It handles the full build chain: ZK binary compilation, WASM building, genesis setup, and contract deployment.

**Location:** `bin/darkfid/src/tests/pipeline.rs`

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  ContractTestingPipeline                      │
├─────────────────────────────────────────────────────────────┤
│  build_zkas_if_needed()  [builds target/release/zkas]       │
│                                                              │
│  BinaryChecker                                               │
│    ├─► check_zkas() → BinaryStatus                         │
│    ├─► check_wasm() → BinaryStatus                         │
│    └─► check_zkbins() → Vec<BinaryInfo>                    │
│                                                              │
│  build_contract() [ZK bins → WASM, full chain]              │
│    ├─► build_zkas_if_needed()                              │
│    ├─► build_zk_bins()  [proof/*.zk → proof/*.zk.bin]      │
│    └─► cargo build --release -p darkfi_{contract}_contract   │
│                                                              │
│  GenesisRunner                                               │
│    ├─► check_genesis() → GenesisStatus                      │
│    └─► ensure_genesis() → GenesisState (stores harness)     │
│                                                              │
│  ContractDeployer                                            │
│    └─► deploy() → ContractId                                 │
└─────────────────────────────────────────────────────────────┘
```

## Build Chain

ZK binaries must be built BEFORE WASM because the WASM compiler bakes in the ZK circuit parameters:

```
1. zkas compiler (target/release/zkas)
       ↓
2. proof/*.zk  ──zkas──►  proof/*.zk.bin
       ↓
3. cargo build --release -p darkfi_{contract}_contract
       ↓
4. darkfi_{contract}_contract.wasm
```

The pipeline automatically handles this chain via `build_contract()`.

## Contract Types

### Native Contracts (Mandatory)

Deployed at genesis - NOT optional:

| Contract | Purpose |
|----------|---------|
| Native Token | Block rewards, fees, burns |
| Deployooor | WASM contract deployment |

### User Contracts (Optional)

Deployed via Deployooor AFTER genesis using pipeline.toml dependency manifests:

```toml
# src/contract/dex/pipeline.toml
name = "dex"
dependencies = ["money_v3"]
```

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
    Ready(GenesisState),           // Genesis complete
    Invalid,                      // Corrupted state
}
```

## API Reference

### `ContractTestingPipeline::new(contract_name, config, ex)`

Create a new pipeline:

```rust
let mut pipeline = ContractTestingPipeline::new("dex", config, &ex).await?;
```

### `pipeline.check_zkas() -> BinaryStatus`

Check if zkas compiler exists.

### `pipeline.build_zkas_if_needed() -> Result<BinaryStatus>`

Build zkas compiler if missing (`cargo build --release -p zkas`).

### `pipeline.check_wasm() -> BinaryStatus`

Check if WASM binary exists and is current.

### `pipeline.check_zkbins() -> Vec<BinaryInfo>`

Check ZK binary status for each `proof/*.zk.bin`.

### `pipeline.build_zk_bins() -> Result<()>`

Build ZK binaries from .zk source files:
- For each `proof/*.zk`, runs `zkas <file>.zk -o <file>.zk.bin`
- Compares modification times to skip already-current binaries

### `pipeline.build_contract() -> Result<BinaryStatus>`

Full build chain:
1. Build zkas compiler if needed
2. Build ZK binaries if needed
3. Build WASM if needed

### `pipeline.check_genesis() -> Result<GenesisStatus>`

Check genesis state.

### `pipeline.ensure_genesis() -> Result<GenesisState>`

Run genesis if needed (stores `GenesisHarness` internally).

### `pipeline.deploy() -> Result<ContractId>`

Deploy contract using stored genesis harness. Automatically deploys dependencies first.

### `pipeline.ensure_ready_and_deploy() -> Result<ContractId>`

One-shot: build (ZK + WASM) + genesis + deploy.

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

This automatically:
1. Builds zkas compiler
2. Builds ZK binaries for money_v3, then dex
3. Builds WASM for money_v3, then dex
4. Runs genesis (NativeToken + Deployooor)
5. Deploys money_v3, then dex

### Step-by-step (for debugging)

```rust
let mut pipeline = ContractTestingPipeline::new("dex", config, &ex).await?;

let report = pipeline.status_report().await;
info!("ZKAS: {:?}", report.zk_status);
info!("WASM: {:?}", report.wasm_status);
info!("ZK bins: {} found", report.zkbin_status.len());
info!("Genesis: {:?}", report.genesis_status);

if matches!(report.wasm_status, BinaryStatus::Missing) {
    pipeline.build_contract().await?;
}

pipeline.ensure_genesis().await?;
let contract_id = pipeline.deploy().await?;
```

### Manual ZK binary building

```rust
let pipeline = ContractTestingPipeline::new("money_v3", config, &ex).await?;
pipeline.build_zkas_if_needed().await?;
pipeline.build_zk_bins().await?;
```

## Dependency Resolution

The pipeline uses `pipeline.toml` manifests to resolve dependencies:

```rust
let manifest = ContractManifest::load("dex")?;
let deps = manifest.resolve_dependencies()?;
// Returns ["money_v3"] for dex
```

Dependencies are deployed in topological order (base contracts first).

## Relationship to GenesisHarness

`ContractTestingPipeline` uses `GenesisHarness` internally for genesis setup:

- `GenesisHarness` = NativeToken + Deployooor baseline only
- `ContractTestingPipeline` = GenesisHarness + binary building + deployment

You can also use `GenesisHarness` directly if you only need the baseline:

```rust
let mut genesis = GenesisHarness::new(config, &ex).await?;
genesis.generate_genesis_blocks(3).await?;
```

## Dependency Management

The pipeline uses `pipeline.toml` manifests for contract dependency declaration, following a package management design philosophy.

### Design Philosophy

**Principle: Explicit Dependencies, Auto-Discovery, Zero Manual Workflow**

1. **Explicit Declaration**: Each contract declares its dependencies in `pipeline.toml`
2. **Auto-Resolution**: Pipeline resolves full dependency tree via topological sort
3. **Zero Manual Workflow**: User only specifies target contract; pipeline handles the rest

### Why This Matters

| Aspect | Without Auto-Discovery | With Auto-Discovery |
|--------|------------------------|---------------------|
| Deploying DEX | Manually deploy MoneyV3 first, then DEX | `pipeline.deploy("dex")` → handles MoneyV3 automatically |
| CI/CD | Script per contract with hardcoded deps | Single pipeline per contract |
| Adding deps | Update deployment scripts | Update `pipeline.toml` |

### pipeline.toml Manifest

```toml
# src/contract/<contract>/pipeline.toml
name = "<contract_name>"
dependencies = ["<dep1>", "<dep2>"]
```

### Dependency Resolution

```rust
let manifest = ContractManifest::load("dex")?;
let deps = manifest.resolve_dependencies()?;
// Returns ["money_v3"] for dex (topological order)
```

The resolver:
1. Loads `pipeline.toml` for target contract
2. Recursively loads dependencies
3. Returns them in deployment order (base contracts first)

### Dependency Graph Example

```
dex
 ├─── money_v3
 │      └── (no dependencies)
 └─── (nothing else)

pipeline.deploy("dex") deploys in order:
  1. money_v3 (base contract)
  2. dex (depends on money_v3)
```

### Adding a New Contract

1. Create `src/contract/new_contract/`
2. Add `pipeline.toml`:
   ```toml
   name = "new_contract"
   dependencies = ["money_v3"]
   ```
3. Pipeline automatically:
   - Builds ZK binaries for dependencies first
   - Builds ZK binaries for new contract
   - Builds WASM for dependencies
   - Builds WASM for new contract
   - Deploys dependencies
   - Deploys new contract

No scripts to update, no ordering to remember.

## File Locations

| File | Purpose |
|------|---------|
| `bin/darkfid/src/tests/pipeline.rs` | ContractTestingPipeline |
| `bin/darkfid/src/tests/genesis.rs` | GenesisHarness |
| `src/contract/*/pipeline.toml` | Contract dependency manifests |
| `src/contract/*/proof/*.zk` | ZK circuit source files |
| `src/contract/*/proof/*.zk.bin` | Compiled ZK binaries |

## See Also

- [Genesis Harness](./genesis_harness.md) - NativeToken + Deployooor baseline only
- [Test Harness Guide](./test_harness_guide.md) - Full testing overview
- [Sync Module](./sync.md) - Stateless block verification
