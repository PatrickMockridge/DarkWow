# Level 2: Heavyweight Tests

Local tests with real ZK proof generation and on-chain execution. Tests contract
**functions, state transitions, and uncle-merkle block execution.** Requires
`--release` mode and increased stack size due to halo2 proving key intensity.

**Deployment is NOT tested here.** Contracts are deployed via the direct
`deploy_contract()` path for setup convenience. Deployment correctness is
tested separately by Level 1 (Lightweight) through the Deployooor contract.

## Demarcation from Level 1 (Lightweight)

| Concern | Level 1 — Lightweight | Level 2 — Heavyweight |
|---------|----------------------|----------------------|
| Deployment path | **Deployooor** (real production flow) | Direct `deploy_contract()` (setup convenience) |
| ZK proofs | None | Required for all calls |
| Contract functions | Not tested | Every endpoint exercised |
| State transitions | Not tested | Verified via `apply_block_with_uncles()` |
| Uncle-merkle blocks | Not tested | Multi-uncle, depth, mixed exec, invalid proof rejection |
| Block gas limits | Not tested | Cumulative gas tracking across calls |
| Cross-contract calls | Not tested | Multi-contract integration (recruitment pipeline) |

**Both are required.**

## What's Covered

| Component | Location | What It Verifies |
|-----------|----------|-----------------|
| HeavyweightPipeline | `bin/dwowd/src/tests/heavyweight_pipeline.rs` | Contract functions, ZK proofs, state transitions, uncle-merkle execution |
| ContractHarness trait | `src/contract/test-harness/src/harness.rs` | Per-contract ZK circuit access |
| Contract harness modules (28) | `src/contract/test-harness/src/harness/` | Proof generation for each contract |

## ContractHarness Trait

Every contract that supports heavyweight testing implements the
`ContractHarness` trait:

```rust
pub trait ContractHarness {
    fn name(&self) -> &str;                        // e.g. "dex", "money_v3"
    fn circuits(&self) -> Vec<&'static str>;       // circuit namespaces
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary>;  // ZK binary
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey>;   // proving key
}
```

Each harness module (e.g., `harness/money_v3.rs`) loads its ZK circuit binaries
via `include_bytes!`, builds `ProvingKey` objects at construction time, and
implements the trait.

**Location:** `src/contract/test-harness/src/harness/`

## HeavyweightPipeline

The `HeavyweightPipeline<H: ContractHarness>` provides full ZK-aware contract
function/endpoint testing. It owns a `GenesisHarness` directly and provides
`exec()`, `exec_as_uncle()`, `exec_mixed()`, and `exec_multi_uncle()` for
on-chain contract calls with real proofs and uncle-merkle block formation.

### Usage

```rust
use dwow_contract_test_harness::harness::{DexHarness, ContractHarness};

let harness = DexHarness::new();
let mut pipeline = HeavyweightPipeline::new(harness, "dex").await?;

// Deploy contract via direct path (setup convenience — not testing deployment)
let wasm = include_bytes!("../../../../src/contract/dex/dwow_dex_contract.wasm");
let contract_id = pipeline.deploy(wasm).await?;

// Execute contract calls with ZK proofs through apply_block_with_uncles()
let result = harness.create_swap(/* params */)?;
pipeline.exec(&result.call_data).await?;
```

### Block Execution Modes

```rust
// Canonical block execution
pipeline.exec(&call_data).await?;

// Execute as an uncle block
pipeline.exec_as_uncle(&call_data).await?;

// Mixed canonical + uncle in one block
pipeline.exec_mixed(&canonical_data, &uncle_data).await?;

// Multiple uncles in one block
pipeline.exec_multi_uncle(&canonical_data, &[uncle1, uncle2, uncle3]).await?;
```

### Running Heavyweight Tests

```bash
# All 36 heavyweight tests (requires --release for halo2 proving keys)
RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 cargo test --release -p dwowd -- test_heavyweight_

# Individual tests
cargo test --release -p dwowd -- test_heavyweight_dao_escrow
cargo test --release -p dwowd -- test_heavyweight_identity

# Contract metadata: deploy with metadata + ZK proofs + state transitions
RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 cargo test --release -p dwowd \
    test_heavyweight_metadata
```

### Why Stack Overflow Occurs

halo2 proof generation uses deep recursion for polynomial arithmetic. When
building multiple proving keys simultaneously, stack usage exceeds the default
~8MB limit. Release mode optimizes this away; alternatively, increase
`RUST_MIN_STACK`. The recommended value of `67108864` (64MB) has been tested
through hundreds of consecutive runs with zero SIGSEGV.

### Test Coverage

37 tests total: 29 contract-specific tests (22 with WASM deploy + 7
harness-only for non-WASM contracts) + 1 cross-contract integration test
(recruitment_pipeline) + 7 block-execution infrastructure tests (canonical,
uncle, mixed, multi-uncle, depth, empty-uncle, invalid-uncle-proof).

## Lightweight vs Heavyweight

| Aspect | Level 1 (Lightweight) | Level 2 (Heavyweight) |
|--------|----------------------|----------------------|
| Purpose | **Deployooor-based deployment** (real production path) | **Contract functions + ZK proofs + uncle-merkle** |
| Deployment | DeployV1 → Deployooor → hook → __initialize | Direct `deploy_contract()` (setup convenience) |
| ContractId | Derived from deploy keypair | Deterministic hash of contract name |
| ZK Proofs | None | Required for all calls |
| Runtime | Seconds | 30-120 seconds per test |
| Mode | Debug or release | Release (or debug with 64MB stack) |

## Contract Harness List

The test harness crate supports 28 contracts. Each has a harness module under
`src/contract/test-harness/src/harness/`:

| Contract | Circuits | Client Module |
|----------|----------|---------------|
| identity | 8 | `src/contract/identity/src/client/` |
| labor_market | 9 | `src/contract/labor_market/src/client/` |
| oracle | 5 | `src/contract/oracle/src/client/` |
| auction | 6 | `src/contract/auction/src/client/` |
| tender | 5 | `src/contract/tender/src/client/` |
| attestation | 8 | `src/contract/attestation/src/client/` |
| subscription | 3 | `src/contract/subscription/src/client/` |
| escrow | 4 | `src/contract/escrow/src/client/` |
| stablecoin | 5 | `src/contract/stablecoin/src/client/` |
| bridge | 6 | `src/contract/bridge/src/client/` |
| dex | 6 | `src/contract/dex/src/client/` |
| atomic_swap | 3 | `src/contract/atomic_swap/src/client/` |

Plus: attestation, auction, baccarat, betting_stake, bridge, darkbet_exchange,
darktoshi_dice, dao_escrow, deployooor, drain_protection, game_room,
insurance_market, lottery, money_v3, native_token, pool_stake,
relayer_endowment, roulette, slot, stablecoin, subscription, tender.

Each client module provides:
- `*PublicInputs` struct with `to_vec()` for circuit public inputs
- `*CallData` struct with private/public input data
- `compute_public_inputs()` method
- `to_witnesses()` method returning `Vec<Witness>` for `ZkCircuit`
- `*_proof()` function that creates `Proof`

## Native vs WASM Contracts

### Native Contracts

Native contracts are compiled into dwowd with static ContractIds defined in the
SDK. VKs are injected at harness initialization. No deployment step needed.

```rust
// In dwow_sdk::crypto
pub static ref DEPLOYOOOR_CONTRACT_ID: ContractId = ContractId::from(...);
pub static ref NATIVE_TOKEN_CONTRACT_ID: ContractId = ContractId::from(...);
```

**Current native contracts:** NativeToken (consensus-first, block rewards +
fees) and Deployooor (WASM deployment). MoneyV2 is deprecated.

### WASM Contracts

WASM contracts are deployed via Deployooor. Their ContractId is derived from
the deploy public key at deployment time.

```rust
let contract_id = ContractId::derive_public(deploy_public_key);
```

**For test harnesses:**
- VKs are injected **after** deployment (not at initialization)
- ContractId is only known after deployment
- Must deploy before use

## Creating a New Contract Harness

**Step 1: Add dependency** in `src/contract/test-harness/Cargo.toml`:
```toml
dwow_<contract>_contract = { path = "../<contract>", features = ["client", "no-entrypoint"] }
```

**Step 2: Add ZK proof bins** in `src/contract/test-harness/src/vks.rs`:
```rust
&include_bytes!("../../<contract>/proof/circuit_v1.zk.bin")[..],
```

**Step 3: Add namespace injection** in `vks.rs::inject()`:
```rust
"<CONTRACT>_CONTRACT_ZKAS_NS" => {
    let key = serialize(&namespace.as_str());
    let value = serialize(&(bincode.clone(), vk.clone()));
    overlay.insert(&contract_db_name, &key, &value)?;
}
```

**Step 4: Create harness module** at
`src/contract/test-harness/src/harness/<contract>.rs` implementing
`ContractHarness`.

**Step 5: Add module** to `src/contract/test-harness/src/lib.rs`.

## File Locations

| Component | Path |
|-----------|------|
| HeavyweightPipeline | `bin/dwowd/src/tests/heavyweight_pipeline.rs` |
| ContractHarness trait | `src/contract/test-harness/src/harness.rs` |
| Contract harness modules (28) | `src/contract/test-harness/src/harness/` |
| VK injection | `src/contract/test-harness/src/vks.rs` |
| Contract client modules | `src/contract/<name>/src/client/` |
| Contract proof sources | `src/contract/<name>/proof/*.zk` |
