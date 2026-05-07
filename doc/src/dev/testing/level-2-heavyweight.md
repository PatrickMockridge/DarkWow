# Level 2: Heavyweight Tests

Local tests with real ZK proof generation and execution. Requires `--release`
mode or increased stack size due to halo2 proving key intensity.

## What's Covered

| Component | Location | What It Verifies |
|-----------|----------|-----------------|
| HeavyweightPipeline | `bin/darkfid/src/tests/heavyweight_pipeline.rs` | Full contract execution with ZK proofs |
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
testing. It owns a `GenesisHarness` directly and provides `exec()` and
`exec_with_children()` for on-chain contract calls with real proofs.

### Usage

```rust
use dwow_contract_test_harness::harness::{DexHarness, ContractHarness};

let harness = DexHarness::new();
let mut pipeline = HeavyweightPipeline::new(harness, "dex", config, ex).await?;

// Generate genesis blocks
pipeline.generate_genesis_blocks(3).await?;

// Deploy contract
let wasm = read_wasm("dex");
let contract_id = pipeline.deploy(wasm).await?;

// Execute contract calls with ZK proofs
pipeline.exec(function_id, call_data, proofs).await?;
```

### Cross-Contract FuncId Binding

When a contract makes child calls (e.g., DEX calling money_v3), FuncIds must
be computed from the deployed contract's real ContractId:

```rust
// Deploy dependency first
let money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", config, ex).await?;
let money_contract_id = money_pipeline.deploy(money_wasm).await?;

// Compute real FuncIds for cross-contract calls
let alice_otc_func_id = compute_func_id(money_contract_id, 0x05);

// Generate proof with real FuncIds
let execute_result = harness.execute_swap(..., alice_otc_func_id, bob_otc_func_id)?;

// Execute with child calls (empty proofs for child calls are placeholders)
pipeline.exec_with_children(0x03, call_data, vec![proof],
    vec![child_call_0, child_call_1], vec![vec![], vec![]]).await?;
```

### Running Heavyweight Tests

**Option 1: Release mode (recommended):**
```bash
cargo test --package dwowd --release test_dex_heavyweight
cargo test --package dwowd --release test_money_v3_heavyweight
```

**Option 2: Increased stack size:**
```bash
export RUST_MIN_STACK=16777216  # 16MB
cargo test --package dwowd test_dex_heavyweight
```

### Why Stack Overflow Occurs

halo2 proof generation uses deep recursion for polynomial arithmetic. When
building multiple proving keys simultaneously, stack usage exceeds the default
~8MB limit. Release mode optimizes this away; alternatively, increase
`RUST_MIN_STACK`.

## Lightweight vs Heavyweight

| Aspect | Level 1 (Lightweight) | Level 2 (Heavyweight) |
|--------|----------------------|----------------------|
| Purpose | Deployment verification | Full contract execution |
| ZK Proofs | None | Required for all calls |
| GenesisHarness | Via pipeline | Owned directly by HeavyweightPipeline |
| Runtime | Seconds | 30-120 seconds per test |
| Mode | Debug or release | Release (or debug with 16MB stack) |

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
| HeavyweightPipeline | `bin/darkfid/src/tests/heavyweight_pipeline.rs` |
| ContractHarness trait | `src/contract/test-harness/src/harness.rs` |
| Contract harness modules (28) | `src/contract/test-harness/src/harness/` |
| VK injection | `src/contract/test-harness/src/vks.rs` |
| Contract client modules | `src/contract/<name>/src/client/` |
| Contract proof sources | `src/contract/<name>/proof/*.zk` |
