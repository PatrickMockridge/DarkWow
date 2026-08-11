# Level 2: Heavyweight Tests

Local tests with real ZK proof generation and on-chain execution through `accept_block`.
Tests contract functions, state transitions, and uncle-merkle block execution. Requires
`--release` mode and increased stack size (`RUST_MIN_STACK=67108864`).

**Normative reference:** [Heavyweight Testing Specification](heavyweight-spec.md) (RFC 2119).
This document is a user-facing guide. Where they conflict, `heavyweight-spec.md` governs.

## Demarcation from Level 1

| Concern | Level 1 — Lightweight | Level 2 — Heavyweight |
|---------|----------------------|----------------------|
| Deployment path | **Deployooor** (real production flow) | Direct `deploy_contract()` (setup convenience) |
| ZK proofs | None | Required for all ZK-gated calls |
| Contract functions | Not tested | Every endpoint exercised through `accept_block` |
| State transitions | Not tested | Verified post-submission |
| Uncle-merkle blocks | Not tested | Multi-uncle, depth, mixed exec, invalid proof rejection |
| Block gas limits | Not tested | Cumulative gas tracking across calls |
| Cross-contract calls | Not tested | Multi-contract integration |

## Coverage Status

### Test Structure

59 tests total across two patterns:
- **32 spec-based tests** using `run_heavyweight_test()` (74.4%): each contract has a
  `ContractTestSpec` in `bin/dwowd/src/tests/specs/` and a 4-line wrapper in
  `heavyweight_pipeline.rs`
- **11 old-pattern tests** (25.6%): 8 block-execution tests (canonical_exec,
  coinbase_rejects_wrong_reward, uncle_exec, mixed_exec, multi_uncle, uncle_depth,
  empty_uncle, invalid_uncle_proof) plus 3 integration tests (metadata,
  recruitment_pipeline, relayer_lifecycle)

### Genesis Contracts (9 contracts, 54 function variants)

| Contract | Functions | In Spec | accept_block | verify_state | Rating |
|----------|-----------|---------|--------------|--------------|--------|
| box | 3 | 2 | 2 | 2/2 | InitializeV1 harness gap |
| purse | 4 | 3 | 3 | 2/3 | InitializeV1 harness gap |
| multisig | 4 | 3 | 3 | 3/3 | InitializeV1 harness gap |
| oracle | 6 | 6 | 6 | 6/6 | Full |
| promissory_note | 6 | 6 | 6 | 6/6 | Full |
| identity | 9 | 11 | 11 | 11/11 | Full (includes mode-specific CreateClaim) |
| attestation | 13 | 13 | 13 | 13/13 | Full |
| deployooor | 2 | 2 | 2 | 1/2 | Full |
| native_token | 7 | 5 | 5 | 5/5 | Pow/FeeCollect exercised structurally |

**Gaps:** box, purse, and multisig lack `InitializeV1` — harnesses don't expose
`initialize()` methods. native_token exercises `PoWRewardV1` and `FeeCollectV1`
structurally through coinbase but lacks explicit endpoint verification.

### WASM Contracts (23 contracts, all migrated)

All 23 WASM contracts have `ContractTestSpec` files registered in
`bin/dwowd/src/tests/specs/mod.rs`. Endpoint population status:

| Tier | Count | Contracts | Endpoints |
|------|-------|-----------|-----------|
| FULL | 12 | auction, baccarat, bearer_bond, betting_stake, darktoshi_dice, dex, lottery, otc_swap, pool_stake, roulette, stablecoin, tender | All active |
| HARVESTABLE | 4 | dao_escrow (12/13), labor_market (9/9), subscription (5/5), escrow (3/3) | Most active, documented gaps |
| UNDERPOWERED | 4 | bridge (7/7), darkbet_exchange (4/10), insurance_market (2/16), relayer_endowment (3/8) | Active endpoints populated, documented harness gaps |
| STUB | 3 | drain_protection (0/9), game_room (0/12), slot (0/4) | All `empty_witnesses` — needs client proof modules |

**Total active endpoints:** 169 across 32 specs. **verify_state closures:** 49
(genesis contracts + multisig). **has_initialize:** 2 (dao_escrow, identity).

### Guardrail Compliance

| Guardrail | Status | Verification |
|-----------|--------|-------------|
| RG-5 (No strict_zk toggling) | PASS | `const STRICT_ZK = true`, no field |
| RG-6 (FeeCollectV1 unconditional) | PASS | Enforced by uniform runner |
| RG-7 (Genesis deploy rejection) | PASS | `deploy()` rejects 9 known names |
| RG-10 (No swallowed failures) | PASS | Zero `println!("skipped")` |
| RG-16 (No compatibility shims) | PASS | Zero compat_/_bridge/_shim methods |
| RG-21 (No heuristic ZK gating) | PASS | `is_zk` from `EndpointSpec`, never heuristic |
| RG-24 (No false positives) | PASS | STUB contracts have 0 endpoints |
| RG-26 (No `#[allow(dead_code)]`) | PASS | Zero in test infrastructure |
| RG-27 (No preserved old bodies) | PASS | Zero `_old_*` functions |
| CI scanner | ACTIVE | `contrib/ci/scan_heavyweight_antipatterns.sh` — 11 patterns |

## Uniform Test Runner

Every spec-based test calls `run_heavyweight_test()`, defined in
`bin/dwowd/src/tests/uniform_runner.rs`. It executes this sequence:

1. **Pre-test integrity checks:** genesis block hash, initial supply, contract existence
2. **Deploy/Resolve ContractId:** WASM contracts deployed via `deploy_router`;
   genesis contracts use static `ContractId`
3. **Initialize** (if `has_initialize`): calls the `initialize` closure
4. **Endpoint loop:** one endpoint per block. Each block: `with_call` →
   `with_fee_collect` → `submit` → height assertion → `verify_state`
5. **Nullifier replay rejection:** replays the first ZK endpoint, expects rejection
6. **Post-test integrity:** block hash chain continuity, supply reconciliation
7. **Determinism:** Pipeline B replays the same scenario, compares block hashes

### ContractTestSpec

```rust
pub struct ContractTestSpec<'a> {
    pub name: &'static str,
    pub is_genesis: bool,
    pub contract_id: ContractId,
    pub harness: &'a dyn ContractHarness,
    pub wasm_bytes: Option<&'static [u8]>,
    pub has_initialize: bool,
    pub initialize: Option<Box<dyn Fn() -> Result<EndpointResult> + 'a>>,
    pub endpoints: Vec<EndpointSpec<'a>>,
    pub needs_coinbase_coordination: bool,  // native_token only
}
```

### EndpointSpec

```rust
pub struct EndpointSpec<'a> {
    pub name: &'static str,                                             // function enum variant name
    pub is_zk: bool,                                                    // authoritative ZK gating
    pub generate: Box<dyn Fn() -> Result<EndpointResult> + 'a>,        // produces call_data + proofs
    pub generate_with_coinbase: Option<Box<dyn Fn(&PrefetchedCoinbase) -> Result<EndpointResult> + 'a>>,
    pub verify_state: Option<Box<dyn Fn(&HeavyweightPipeline) -> Result<()> + 'a>>,
}
```

ZK gating uses `EndpointSpec::is_zk` — authoritative contract metadata, never a heuristic.
The uniform runner's `submit_block()` enforces the ZK gate BEFORE calling `with_call()`.

### Spec File Pattern

Each contract has a spec file at `bin/dwowd/src/tests/specs/<contract>_spec.rs` exporting
a single function:

```rust
pub fn <contract>_test_spec() -> ContractTestSpec<'static> { ... }
```

Specs using simple endpoints import `mk_ep` from `specs/helpers.rs`. Specs needing
`verify_state` closures or `generate_with_coinbase` construct `EndpointSpec` directly.

The heavyweight_pipeline.rs wrapper is exactly 4 lines:

```rust
#[test]
fn test_heavyweight_<contract>() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::<contract>_spec::<contract>_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&<contract>_test_spec()))?)
}
```

## Module Architecture

The uniform runner is decomposed into 12 shared modules under `bin/dwowd/src/tests/modules/`:

| Module | Lines | Responsibility | Spec Ref |
|--------|-------|---------------|----------|
| `chain_setup` | 15 | Initialize test HeavyweightPipeline | — |
| `block_submission` | 37 | `submit_single_call_block` with ZK gating | §7.2 PR-6 |
| `endpoint_exercise` | 62 | Exercise endpoint + verify state | §6 |
| `coinbase_coordination` | 66 | Prefetch coinbase params (native_token only) | §5.1 |
| `integrity_checks` | 67 | Pre/post-test integrity verification | §5.2/5.3 |
| `nullifier_replay` | 33 | Nullifier replay rejection | §3.6 |
| `deploy_router` | 30 | Deploy WASM or resolve genesis ContractId | RG-7 |
| `determinism` | 49 | Pipeline B replay for hash comparison | §3.7 |
| `uncle_helpers` | 105 | Uncle block construction + shared block-exec helpers | §8 |
| `witness_helpers` | 70 | Witness construction utilities | — |
| `error_bridge` | 13 | Error type bridging (`Box<dyn Error>` → `dwow_core::Error`) | — |
| mod.rs | 14 | Module declarations | — |

## HeavyweightPipeline

`HeavyweightPipeline` owns chain state, cached ZK coinbase keys, and a deterministic
test mining key. Created once per test. Every block built through it includes:
PoWRewardV1 (coinbase) → contract calls → FeeCollectV1.

### ZK Gating

ZK proof enforcement uses `const STRICT_ZK: bool = true` — immutable and structural.
The uniform runner's `submit_block()` function checks `EndpointSpec::is_zk` (authoritative
metadata) and rejects empty proofs BEFORE calling `with_call()`. `with_call()` accepts
proofs without validation — it is a data accumulation method, not a security gate (§7.2 PR-6).

### State Inspection API

```rust
pipeline.query_contract_state(contract_id, tree_name, key) -> Result<Option<Vec<u8>>>
pipeline.cumulative_supply() -> Result<u64>
pipeline.block_hash_chain_continuous() -> Result<bool>
pipeline.block_hash_at(height) -> Result<blake3::Hash>
```

### Usage

```rust
let pipeline = HeavyweightPipeline::new().await?;
pipeline.init_genesis().await?;
let harness = DexHarness::spawn();
let wasm = include_bytes!("../../../../src/contract/dex/dwow_dex_contract.wasm");
let contract_id = pipeline.deploy(&harness, "dex", wasm).await?;

let result = harness.create_swap(/* params */)?;
let block = pipeline.block()?;
block.with_call(contract_id, &harness, &result.call_data, vec![result.proof])?;
block.with_fee_collect()?;
block.submit().await?;
```

## Running Heavyweight Tests

```bash
# Single contract
./heavyweight.sh --dex

# Block execution suite
./heavyweight.sh --block-execution

# All 59 tests
./heavyweight.sh --all
```

Raw cargo command:
```bash
RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 cargo test --release -p dwowd -- test_heavyweight_
```

## ContractHarness Trait

```rust
pub trait ContractHarness {
    fn name(&self) -> &str;
    fn circuits(&self) -> Vec<&'static str>;
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary>;
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey>;
    fn verify_zk_coverage(&self) -> Result<()> { /* default impl */ }
    fn non_zk_functions(&self) -> &'static [u8] { &[] }
    fn state_trees(&self) -> &'static [&'static str] { &[] }
    fn function_count(&self) -> usize { self.circuits().len() }
}
```

32 harness files in `src/contract/test-harness/src/harness/`. One harness (box.rs)
overrides `non_zk_functions`, `state_trees`, and `function_count`. The remaining 31
use trait defaults — a documented gap tracked for future standardization.

## Creating a New Contract Test

1. Add harness module at `src/contract/test-harness/src/harness/<contract>.rs`
2. Implement `ContractHarness` trait
3. Register in `src/contract/test-harness/src/lib.rs`
4. Create spec file at `bin/dwowd/src/tests/specs/<contract>_spec.rs`
5. Register in `bin/dwowd/src/tests/specs/mod.rs`
6. Add 4-line wrapper to `bin/dwowd/src/tests/heavyweight_pipeline.rs`

## File Locations

| Component | Path |
|-----------|------|
| HeavyweightPipeline + tests | `bin/dwowd/src/tests/` |
| Uniform runner | `bin/dwowd/src/tests/uniform_runner.rs` |
| Spec files (32) | `bin/dwowd/src/tests/specs/` |
| Shared modules (12) | `bin/dwowd/src/tests/modules/` |
| ContractHarness trait | `src/contract/test-harness/src/harness.rs` |
| Harness modules (32) | `src/contract/test-harness/src/harness/` |
| CI anti-pattern scanner | `contrib/ci/scan_heavyweight_antipatterns.sh` |
| CI coverage checker | `contrib/ci/check_heavyweight_coverage.sh` |
| CI compilation checker | `contrib/ci/check_compiles.sh` |
| Spec (normative) | `doc/src/dev/testing/heavyweight-spec.md` |

## Contract Harness List

| Contract | Circuits | Client Module |
|----------|----------|---------------|
| attestation | 5 | `src/contract/attestation/src/client/` |
| auction | 6 | `src/contract/auction/src/client/` |
| baccarat | 2 | `src/contract/baccarat/src/client/` |
| bearer_bond | 4 | `src/contract/bearer_bond/src/client/` |
| betting_stake | 5 | `src/contract/betting_stake/src/client/` |
| box | 2 | `src/contract/box/src/client/` |
| bridge | 2 | `src/contract/bridge/src/client/` |
| dao_escrow | 6 | `src/contract/dao_escrow/src/client/` |
| darkbet_exchange | 4 | `src/contract/darkbet_exchange/src/client/` |
| darktoshi_dice | 2 | `src/contract/darktoshi_dice/src/client/` |
| deployooor | 0 | (pure WASM, no ZK) |
| dex | 4 | `src/contract/dex/src/client/` |
| drain_protection | 1 | `src/contract/drain_protection/src/client/` |
| escrow | 4 | `src/contract/escrow/src/client/` |
| game_room | 5 | `src/contract/game_room/src/client/` |
| identity | 8 | `src/contract/identity/src/client/` |
| insurance_market | 2 | `src/contract/insurance_market/src/client/` |
| labor_market | 7 | `src/contract/labor_market/src/client/` |
| lottery | 2 | `src/contract/lottery/src/client/` |
| multisig | 3 | `src/contract/multisig/src/client/` |
| native_token | 3 | `src/contract/native_token/src/client/` |
| oracle | 1 | `src/contract/oracle/src/client/` |
| otc_swap | 4 | `src/contract/otc_swap/src/client/` |
| pool_stake | 4 | `src/contract/pool_stake/src/client/` |
| promissory_note | 4 | `src/contract/promissory_note/src/client/` |
| purse | 3 | `src/contract/purse/src/client/` |
| relayer_endowment | 3 | `src/contract/relayer_endowment/src/client/` |
| roulette | 2 | `src/contract/roulette/src/client/` |
| slot | 2 | `src/contract/slot/src/client/` |
| stablecoin | 8 | `src/contract/stablecoin/src/client/` |
| subscription | 3 | `src/contract/subscription/src/client/` |
| tender | 4 | `src/contract/tender/src/client/` |

## Guardrail Registry

Guardrails are mechanical verification rules. Each has a binary check — no agent
self-assessment.

| RG | Rule | Verification |
|----|------|-------------|
| RG-5 | No `strict_zk` toggling | `const STRICT_ZK = true`, no field on HeavyweightPipeline |
| RG-6 | FeeCollectV1 unconditional | `with_fee_collect()` always appends |
| RG-7 | Genesis deploy rejection | `deploy()` rejects known genesis ContractIds |
| RG-8 | State inspection API | `query_contract_state()`, `cumulative_supply()` |
| RG-9 | Zkbin freshness check | `check_zkbin_freshness.sh` before test runs |
| RG-10 | No swallowed failures | Zero `println!("skipped")`, zero match-Err-skip |
| RG-16 | No compatibility shims | `grep compat_\|_compat\|legacy_\|_bridge\|_shim` → 0 |
| RG-17 | No breaking API without migration | `cargo check` passes after every commit |
| RG-18 | Compilation checkpoint | `check_compiles.sh` in compliance report |
| RG-19 | Clean working tree | `git status --porcelain` empty at phase end |
| RG-21 | No heuristic ZK gating | `is_zk` from `EndpointSpec`, never `proofs.is_empty()` |
| RG-22 | Rip out drifted code | 3+ prohibited patterns → replace, don't patch |
| RG-24 | No false positives | Zero `empty_witnesses` in active specs |
| RG-25 | No stopping mid-migration | Complete phase before status reports |
| RG-26 | No `#[allow(dead_code)]` | Zero in test infrastructure |
| RG-27 | Old bodies deleted | Zero `_old_*` functions, git IS provenance |
| RG-28 | No empty shell functions | `body deleted` comments → delete declaration too |

Additional guardrails (RG-0 through RG-4, RG-11 through RG-15, RG-20, RG-23) are
referenced in code comments and phase compliance reports but lack formal definitions
in the current documentation. See `heavyweight-spec.md` §10 for the compliance
checklist covering many of these.

## Migration History

### Phase 0 — Baseline (2026-08-05)

Anti-pattern scan revealed 24 violations: 13 match-Err-skip, 4 ZK-proof-only, 1
comment-deferred, 4 explicit-skip, 2 strict_zk-toggling. Coverage: 35/54 genesis
functions (65%), 19 gaps. Multiple tests skipped `accept_block` entirely.

### Phase 1 — Uniform Runner Infrastructure (2026-08-05)

- `strict_zk` field removed from `HeavyweightPipeline` (replaced by `const STRICT_ZK`)
- `FeeCollectV1` made unconditional (RG-6)
- State inspection API added: `query_contract_state()`, `cumulative_supply()`,
  `block_hash_chain_continuous()`, `block_hash_at()`
- `with_call()` signature simplified to 4 parameters; ZK gating moved to
  `submit_single_call_block()` in `block_submission.rs` (PR-6)
- Genesis deploy rejection added: `deploy()` rejects 9 known genesis names (RG-7)
- Uniform runner created: `ContractTestSpec`, `EndpointSpec`, `run_heavyweight_test()`
- Design document: `uniform-runner-design.md` (now merged into this document)

### Phase 2 — Harness Standardization (Partial, 2026-08-05)

- Circuit name normalization: `FunctionNameV2` convention (box, purse, promissory_note)
- `state_trees()`, `non_zk_functions()`, `function_count()` added to `ContractHarness`
- BoxHarness: first to override all three trait defaults
- 31 harnesses still use trait defaults — tracked as future standardization work

### Phase 3 — WASM Migration (2026-08-05)

All 23 WASM contracts migrated to spec-based uniform runner. Old test bodies deleted
entirely (RG-27). ~1200 lines of `#[allow(dead_code)]` dead code eliminated. CI
scanner updated with Patterns 10-11 (dead_code and `_old_*` detection). 6 contracts
had endpoint closures populated (insurance_market, darkbet_exchange, labor_market,
dao_escrow, subscription, bridge). 3 STUB contracts correctly have 0 active endpoints
per RG-24.
