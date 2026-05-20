# Level 1: Lightweight Tests

Local tests that run in seconds. No ZK proofs, no P2P networking, no Docker.

## What's Covered

| Test Type | Location | What It Verifies |
|-----------|----------|-----------------|
| ZK circuit compilation | `src/contract/<name>/tests/zk_circuit_test.sh` | `.zk` files compile to `.zk.bin` via zkas |
| Contract integration | `src/contract/<name>/tests/integration.rs` | Serialization, enums, constants, derive determinism |
| GenesisHarness | `bin/dwowd/src/tests/genesis.rs` | Baseline chain: NativeToken + Deployooor |
| Lightweight pipeline | `bin/dwowd/src/tests/pipeline.rs` | Contract deployment (no ZK proofs) |

## ZK Circuit Tests

Each contract has a bash script that compiles its ZK circuits. These verify
that `.zk` source files are syntactically valid and produce `.zk.bin` output.

**Run one:**
```bash
bash src/contract/baccarat/tests/zk_circuit_test.sh
```

**Run all:**
```bash
for script in src/contract/*/tests/zk_circuit_test.sh; do
    bash "$script"
done
```

**Template for new contracts:**
```bash
#!/bin/bash
set -e

ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/<name>/proof"
OUTPUT_DIR="src/contract/<name>/proof"

echo "=== <Name> Contract ZK Circuit Compilation ==="

$ZKAS_BIN ${PROOF_DIR}/<circuit>_v1.zk -o ${OUTPUT_DIR}/<circuit>_v1.zk.bin

# Verify output exists
for circuit in <circuit_list>; do
    [ -f "${OUTPUT_DIR}/${circuit}_v1.zk.bin" ] || exit 1
done
```

## Contract Integration Tests

Integration tests verify the data model and serialization layer without
touching ZK proofs or blockchain state.

**Run a specific contract:**
```bash
cargo test -p dwow_baccarat_contract --test integration
```

**Run all integration tests:**
```bash
cargo test --workspace --test integration
```

### Standard Test Patterns

**Function enum validity:**
```rust
#[test]
fn test_function_enum_valid() {
    assert!(ContractFunction::try_from(0x00).is_ok());
    assert!(ContractFunction::try_from(0x01).is_ok());
}

#[test]
fn test_function_enum_invalid() {
    assert!(ContractFunction::try_from(0xFF).is_err());
}
```

**State enum conversion:**
```rust
#[test]
fn test_state_from_u8() {
    assert_eq!(MyState::try_from(0).unwrap(), MyState::Created);
    assert_eq!(MyState::try_from(1).unwrap(), MyState::Active);
    assert!(MyState::try_from(255).is_err());
}
```

**Serialization round-trip:**
```rust
use dwow_serial::{serialize, deserialize};

#[test]
fn test_params_encoding() {
    let params = ParamsV1 { id: pallas::Base::from(1), value: pallas::Base::from(100) };
    let encoded = serialize(&params);
    let decoded: ParamsV1 = deserialize(&encoded).unwrap();
    assert_eq!(decoded.id, params.id);
    assert_eq!(decoded.value, params.value);
}
```

**Derivation determinism:**
```rust
#[test]
fn test_derive_id() {
    let id = MyStruct::derive_id(pub_x, pub_y, "title");
    let id2 = MyStruct::derive_id(pub_x, pub_y, "title");
    assert_eq!(id, id2);
}
```

### Serialization API

Use `serialize()`/`deserialize()` from `dwow_serial`:

```rust
// Correct
use dwow_serial::{serialize, deserialize};
let encoded = serialize(&params);
let decoded: ParamsV1 = deserialize(&encoded).unwrap();

// Wrong (old API)
let encoded = params.encode().unwrap();
let decoded = ParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();
```

### Field Constructors

```rust
pallas::Base::zero()    // not pallas::Base::ZERO
pallas::Base::one()     // not pallas::Base::ONE
pallas::Base::from_u64(42)
Group::identity()       // not pallas::Point::identity()
PublicKey::from_secret(secret_key)
```

### Common Imports

```rust
use dwow_serial::{deserialize, serialize};
use dwow_sdk::{
    crypto::{pasta_prelude::*, ContractId},
    error::{ContractError, ContractResult},
    pasta, ContractCall,
};
use dwow_<name>_contract::{
    model::{ParamsV1, UpdateV1},
    ContractFunction,
};
```

## GenesisHarness

GenesisHarness provides a reusable baseline chain with the two mandatory
consensus contracts: NativeToken and Deployooor. It does NOT deploy WASM
contracts — use ContractTestingPipeline for that.

**Location:** `bin/dwowd/src/tests/genesis.rs`

### Quick Start

```rust
use crate::tests::genesis::GenesisHarness;

let config = HarnessConfig {
    pow_target: 20,
    pow_fixed_difficulty: Some(BigUint::one()),
    confirmation_threshold: 1,
    max_forks: 8,
    alice_url: "tcp+tls://127.0.0.1:18440".to_string(),
    bob_url: "tcp+tls://127.0.0.1:18441".to_string(),
};

let mut genesis = GenesisHarness::new(config, &ex).await?;
genesis.generate_genesis_blocks(3).await?;
// Chain is ready with NativeToken + Deployooor
```

### API

| Method | Purpose |
|--------|---------|
| `GenesisHarness::new(config, ex)` | Initialize harness with alice/bob nodes and fork |
| `generate_genesis_blocks(n)` | Produce `n` blocks with NativeToken PoW rewards |
| `deploy_contract(wasm, name)` | Deploy a WASM contract via Deployooor, returns `ContractId` |
| `verify_and_apply(block)` | Verify a block via sync module and apply to fork |
| `block_height()` | Get current block height |

### GenesisHarness Architecture

```
GenesisHarness
  ├── harness: Harness
  │     ├── alice: DarkfiNodePtr  (validator + p2p)
  │     └── bob: DarkfiNodePtr
  ├── fork: Fork
  │     ├── overlay: BlockchainOverlay
  │     └── diffs: Vec<StateDiff>
  ├── keypair: Keypair
  └── deployed_contracts: Vec<ContractId>

Native Contracts (pre-deployed at genesis):
  NATIVE_TOKEN_CONTRACT_ID  — block rewards, fees, burns
  DEPLOYOOOR_CONTRACT_ID    — WASM contract deployment
```

## ContractTestingPipeline (Lightweight)

The lightweight pipeline handles the full build chain (ZK binaries, WASM
compilation, genesis setup, deployment) without generating ZK proofs.

**Location:** `bin/dwowd/src/tests/pipeline.rs`

### One-Shot Usage

```rust
let config = HarnessConfig { /* ... */ };
let mut pipeline = ContractTestingPipeline::new("dex", config, &ex).await?;
let contract_id = pipeline.ensure_ready_and_deploy().await?;
```

This automatically:
1. Builds zkas compiler
2. Builds ZK binaries
3. Builds WASM
4. Runs genesis (NativeToken + Deployooor)
5. Deploys the contract (and its dependencies)

### Step-by-Step Usage

```rust
let mut pipeline = ContractTestingPipeline::new("dex", config, &ex).await?;

// Check status of everything
let report = pipeline.status_report().await;
info!("WASM: {:?}", report.wasm_status);
info!("Genesis: {:?}", report.genesis_status);

// Build only what's needed
if matches!(report.wasm_status, BinaryStatus::Missing) {
    pipeline.build_contract().await?;
}

pipeline.ensure_genesis().await?;
let contract_id = pipeline.deploy().await?;
```

### Running Pipeline Tests

```bash
# Test a specific contract (default: dex)
cargo test --package dwowd test_pipeline

# Test via env var
CONTRACT_NAME=money_v3 cargo test --package dwowd test_pipeline
CONTRACT_NAME=stablecoin cargo test --package dwowd test_pipeline
CONTRACT_NAME=dao_escrow cargo test --package dwowd test_pipeline

# Batch deploy all 25+ contracts
cargo test --package dwowd test_all_contracts_deploy
```

## Build Chain

The pipeline handles the build dependency chain automatically:

```
1. zkas compiler (target/release/zkas)
       ↓
2. proof/*.zk  ──zkas──►  proof/*.zk.bin
       ↓
3. cargo build --release -p dwow_{contract}_contract
       ↓
4. dwow_{contract}_contract.wasm
       ↓
5. cp .wasm → contract src dir (for include_bytes!)
       ↓
6. Contract deployed via Deployooor at runtime
```

ZK binaries must be built BEFORE WASM because the WASM compiler embeds the
circuit parameters. The pipeline's `build_contract()` handles this chain.

## Dependency Resolution

Contracts declare dependencies for topological deployment ordering. The
pipeline resolves these at deploy time so base contracts are deployed first.

```
dex ──depends on──► money_v3

pipeline.deploy("dex") deploys in order:
  1. money_v3 (base contract)
  2. dex (depends on money_v3)
```

## Debugging

### Library vs Test Issues

```bash
cargo build -p dwow_<name>_contract          # If this succeeds, issue is in tests
cargo test -p dwow_<name>_contract --test integration
```

### Common Errors

**`db_get` returns `Option<Vec<u8>>`, not the struct directly:**
```rust
let data = wasm::db::db_get(db, &key)?;
let obj: MyStruct = match data {
    Some(bytes) => deserialize(&bytes)?,
    None => return Err(ContractError::NotFound.into()),
};
```

**`process_update` must use function enum matching, not DarkLeaf iteration:**
```rust
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match MyFunction::try_from(update_data[0])? {
        MyFunction::DoSomethingV1 => {
            let update: DoSomethingUpdateV1 = deserialize(&update_data[1..])?;
            Ok(())
        }
    }
}
```

## File Locations

| Component | Path |
|-----------|------|
| GenesisHarness | `bin/dwowd/src/tests/genesis.rs` |
| ContractTestingPipeline | `bin/dwowd/src/tests/pipeline.rs` |
| Harness (2-node base) | `bin/dwowd/src/tests/harness.rs` |
| ZK circuit tests | `src/contract/<name>/tests/zk_circuit_test.sh` |
| Integration tests | `src/contract/<name>/tests/integration.rs` |
| Contract proof sources | `src/contract/<name>/proof/*.zk` |
| Contract WASM binaries | `src/contract/<name>/dwow_<name>_contract.wasm` |
