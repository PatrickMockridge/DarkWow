# Level 1: Lightweight Tests

Local tests that run in seconds. No ZK proofs, no P2P networking, no Docker.

**Primary purpose:** Test contract deployment through the **Deployooor contract** —
the real production path. This is the key difference from Level 2 (Heavyweight)
which tests contract functions and endpoints.

## Demarcation from Level 2 (Heavyweight)

| Concern | Level 1 — Lightweight | Level 2 — Heavyweight |
|---------|----------------------|----------------------|
| Deployment path | **Deployooor** (real production flow) | Direct `deploy_contract()` (setup convenience) |
| ContractId origin | Derived from deploy keypair (`ContractId::derive_public`) | Deterministic hash of contract name |
| Init params | Real serialized params via `DeployParamsV1.ix` | Empty `ix` (contract defaults) |
| ZK proofs | None | Required for all calls |
| Contract functions | Not tested | Every endpoint exercised |
| Uncle-merkle blocks | Not tested | Multi-uncle, depth, mixed exec |

**Both are required.** Level 1 verifies the Deployooor deployment pipeline works
end-to-end. Level 2 verifies contract functions, state transitions, and
uncle-merkle block execution using the direct deploy path for test setup
convenience.

Level 2 also enforces **pre-deploy ZK coverage verification** via
`ContractHarness::verify_zk_coverage()` — if a harness loads a `.zk.bin` but
forgets to list it in `circuits()`, or lists a circuit without loading its
binary, the deploy step fails with a descriptive error. A CI audit test
(`src/contract/test-harness/tests/zk_audit.rs`) decodes all 99 harness-loaded
`.zk.bin` files in under a second on every push, catching mismatches before
they reach production.

## What's Covered

| Test Type | Location | What It Verifies |
|-----------|----------|-----------------|
| ZK circuit compilation | `src/contract/<name>/tests/zk_circuit_test.sh` | `.zk` files compile to `.zk.bin` via zkas |
| Contract integration | `src/contract/<name>/tests/integration.rs` | Serialization, enums, constants, derive determinism |
| GenesisHarness | `bin/dwowd/src/tests/genesis.rs` | Baseline chain: NativeToken + Deployooor pre-deployed |
| Lightweight pipeline | `bin/dwowd/src/tests/pipeline.rs` | **Deployooor-based deployment** (real production path) |

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

GenesisHarness provides a reusable baseline chain with all 9 genesis
contracts pre-deployed. Creates a temp sled database and CChainState
with instant-PoW target (`u32::MAX`).

**Location:** `bin/dwowd/src/tests/genesis.rs`

### Quick Start

```rust
use crate::tests::genesis::GenesisHarness;

let genesis = GenesisHarness::new()?;
// Chain is ready with NativeToken + Deployooor pre-deployed via deploy_contract()
// PoW target is u32::MAX so any nonce passes — instant blocks for tests.
```

### API

| Method | Purpose |
|--------|---------|
| `GenesisHarness::new()` | Create temp sled DB, CChainState; deploy 9 genesis contracts via `set_contract_data()` |
| `deploy_contract(wasm, contract_id, ix)` | Deploy WASM directly (bypasses Deployooor — used by heavyweight tests) |
| `block_height()` | Get current block height |

### GenesisHarness Architecture

```
GenesisHarness
  ├── db: Arc<sled::Db>             — temp sled database
  └── chain_state: Arc<CChainState> — single authoritative chain state (sled, consensus, VMs)

All 9 genesis contracts are stored via `set_contract_data()` at construction.
See [Genesis Contracts](../../arch/genesis.md) for the contract list.
```

## ContractTestingPipeline (Lightweight)

The lightweight pipeline tests contract deployment through the **Deployooor
contract** — the real production path. It builds a `DeployV1` transaction with
`DeployParamsV1` (WASM bincode, public key, init params), submits it through
`apply_block_with_uncles()`, and verifies the contract is deployed and
initialized correctly.

**Location:** `bin/dwowd/src/tests/pipeline.rs`

### Deployment Flow

```
1. Load pre-built WASM via include_bytes!
       ↓
2. Generate deploy keypair (fresh SecretKey)
       ↓
3. Derive ContractId from public key (ContractId::derive_public)
       ↓
4. Build DeployV1 call data: [0x00] + encode(DeployParamsV1 { wasm_bincode, public_key, ix })
       ↓
5. Build transaction targeting DEPLOYOOOR_CONTRACT_ID
       ↓
6. Submit via apply_block_with_uncles() — the production code path
       ↓
7. Deployooor validates WASM exports, checks lock status, records contract_id
       ↓
8. Post-processing hook stores WASM and calls __initialize(ix)
```

### One-Shot Usage

```rust
let mut pipeline = ContractTestingPipeline::new("dex").await?;
let contract_id = pipeline.ensure_ready_and_deploy().await?;
```

This automatically:
1. Creates GenesisHarness (NativeToken + Deployooor pre-deployed)
2. Loads the contract's pre-built WASM
3. Generates a deploy keypair and derives the ContractId
4. Builds and submits a DeployV1 transaction through `apply_block_with_uncles()`
5. Deployooor validates the WASM, records the contract, and triggers `__initialize`

### Running Pipeline Tests

```bash
# Test a specific contract (default: dex)
cargo test -p dwowd test_pipeline

# Test via env var
CONTRACT_NAME=promissory_note cargo test -p dwowd test_pipeline
CONTRACT_NAME=stablecoin cargo test -p dwowd test_pipeline

# Batch deploy all 21 contracts through Deployooor
cargo test -p dwowd test_all_contracts_deploy

# Contract metadata: deploy through Deployooor with metadata in ix
cargo test -p dwowd test_metadata_deploy_lightweight
```

### WASM Staleness Warning

`GenesisHarness::new()` and `ContractTestingPipeline` embed contract WASM
binaries at compile time via `include_bytes!()`. If contract WASM files
are not rebuilt before running `cargo test`, tests execute against stale
binaries.

**Failure mode:** A contract source change updates the WASM `__initialize`
logic, but the pre-built `.wasm` file is not regenerated. The test compiles
and runs successfully, but the stale WASM binary produces a different state
root hash. This manifests as a spurious test failure — the correct fix is
to rebuild WASM, not to update the test's expected hash.

**Prevention:**
```bash
# Before running pipeline tests, rebuild all contract WASMs:
cd src/contract
for contract in */; do
    (cd "$contract" && cargo build --release --target wasm32-unknown-unknown)
done

# Or, for a single contract:
cargo build -p dwow_native_token_contract --release --target wasm32-unknown-unknown

# Then run tests:
cargo test -p dwowd test_pipeline
```

### Available Contracts

21 deployable contracts (all have `dwow_*.wasm`):

```
attestation, auction, bridge, dao_escrow, deployooor, dex,
drain_protection, escrow, game_room, identity, insurance_market,
labor_market, promissory_note, native_token, oracle, pool_stake,
relayer_endowment, slot, stablecoin, subscription, tender
```

6 contracts (baccarat, betting_stake, darkbet_exchange,
darktoshi_dice, lottery, roulette) have `dwow_*.wasm` only — their
`dwow_*.wasm` has not yet been built. These are tested via heavyweight
harness proof generation only.

## Build Chain

WASM binaries are pre-built and loaded via `include_bytes!`. The build chain:

```
1. proof/*.zk  ──zkas──►  proof/*.zk.bin
       ↓
2. cargo build --release --target wasm32-unknown-unknown -p dwow_{contract}_contract
       ↓
3. dwow_{contract}_contract.wasm  ──copy──►  src/contract/<name>/
       ↓
4. include_bytes! in pipeline.rs loads at compile time
       ↓
5. DeployV1 transaction submits WASM through Deployooor at test runtime
```

ZK binaries must be built BEFORE WASM because the WASM compiler embeds the
circuit parameters. WASM files are gitignored — each developer builds them
locally.

## Debugging

### Library vs Test Issues

```bash
cargo build -p dwow_<name>_contract --target wasm32-unknown-unknown
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
