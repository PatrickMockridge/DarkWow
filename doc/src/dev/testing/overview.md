# Testing Overview

DarkWow's testing infrastructure is a key differentiator from upstream DarkFi.
Where upstream provides no structured testing taxonomy and no containerized
development environments, DarkWow ships with four distinct testing levels,
each with dedicated tooling, Docker configurations, and documentation. This
infrastructure was built to support the "fork, build, and customize" development
model: clone the repo, run `cargo test` for fast iteration, then scale up through
the testing levels as your contract matures.

For a developer-oriented entry point, see the [Developer Quick Start Guide](../quickstart.md).

DarkWow has four testing levels, three local (developer machine) and one
public-facing (LAN/internet).

## Quick Reference

| Level | Name | Scope | Runtime | Commands |
|-------|------|-------|---------|----------|
| 1 | Lightweight | Unit tests, integration tests (all 32 contracts), **Deployooor-based deployment** (real production path, no ZK), encode/decode round-trip tests | Seconds | `cargo test`, `cargo test -p dwowd test_pipeline`, `cargo test -p <contract> --test integration` |
| — | **Python Simulations** | **Contract state machines, authorization flows, business rules, edge cases** (no ZK, no crypto — pure logic) | Milliseconds | `python3 -c "from sim.contracts..."` |
| — | **Python Consensus Models** | **Block production, PoW, uncle-merkle, chain reorg, VM concurrency, finality, merge mining** (1:1 Rust specification, 34/34 tests, 8 VM scenarios) | Milliseconds | `python3 contrib/model/chain_validation_model.py`, `python3 contrib/model/vm_state_model.py`, `python3 contrib/model/merge_mining_model.py` |
| 1.5 | **Pre-Production Bridge** | **Production-path integration: real ZK proofs, real AEAD encryption, real accept_block, real wallet scan** (BlockTarget::MAX, deterministic ZK). Enforces MoC gate before Docker pipeline. Nullifier replay + scan determinism verified. | ~2-7 min | `cargo test --release -p dwowd --lib -- test_wallet_coinbase_scan_only test_canonical_call_failure_rejects_block test_merge_mined_block_acceptance test_merge_mined_block_deterministic test_wallet_capability_scan` |
| 2 | Heavyweight | Contract **functions, ZK proofs, uncle-merkle block execution** — all 32 contracts with exhaustive function coverage. Deployment not tested (uses direct path for setup). | Minutes | `./bin/dwowd/src/tests/heavyweight.sh --all`, `cargo test --release -p dwowd test_<contract>_heavyweight` |
| 3 | Containerized Localnet | Multi-node Docker testnet (seed + mining nodes), P2P, RandomX, bridge lifecycle, wallet, 6 modes, 21 phases, composable flags | Persistent | `./test_pipeline.sh --mode native\|merge\|bridge\|wallet\|join-native\|join-merge` in `contrib/docker/darkwow-testnet/` |
| 4 | Containerized Devnet | Public-facing mining node for shared devnets over LAN/internet | Persistent | `docker run --network=host -e IS_SEED=true darkwow-devnet` |
| Wallet | Wallet capabilities | L1: Bash CLI (seconds). L2: Rust in-process (20 tests, <2s). L3: Docker container (persistent). | Seconds to Persistent | `./bin/dww/test_capability_lightweight.sh`, `cargo test -p dwow_wallet --lib -- capability::tests`, `./contrib/docker/darkwow-testnet/test-wallet.sh` |
| Wallet | Wallet in Dockernet | End-to-end wallet testing with mining nodes + wallet container. Guardrailed commands, verified subcommand syntax, pre-flight checklist. | Persistent | See [Wallet Testing in Dockernet](wallet-testing.md) |
| **Fee System** | **Cross-stack coordination** | **Invariant-driven integrative testing across wallet→mempool→miner→contract. Risk factor emergence, encrypted fee channel, PID congestion control, Pedersen accumulator lifecycle. Python model as executable specification (70 tests).** | Seconds to Persistent | `python3 contrib/model/fee_window_model.py`, `cargo test -p dwowd --lib -- fee`, see [Fee System Testing](fee-testing.md) |

**Fee system testing note:** The fee signalling system has no isolated components.
Testing follows an invariant-driven model defined in `fee-spec.md` §14. Tests verify
invariants across their natural scope — wallet→mempool→miner→contract — not functions
in isolation. See [Fee System Testing](fee-testing.md) for the full testing
specification and invariant coverage matrix.

## Production Fidelity

Level 3 (Containerized Localnet) and Level 4 (Containerized Devnet) are
**production test infrastructure**, not developer convenience tools. The
pipeline uses real RandomX PoW at production difficulty, real P2P networking
over TLS, real Docker containers with full build-from-source, real merge
mining with xmrig+p2pool+monerod sidecars, and real 120-second block times.
No mocked components. No simulated network conditions. No RPC shortcuts.

This means the pipeline is **slow by design** — a full native-mode run is
20-40 minutes. Builds clone from origin and compile 30+ WASM contracts with
ZK proof regeneration. Block production waits on actual PoW. Merge mining
receipt verification polls real xmrig stratum output for up to 30 minutes.

**Why:** This is the 1/8 scale model of the production network. If a contract
can hold material value at mainnet, it must be tested in conditions that
faithfully reproduce mainnet behavior. Shortcuts in the test infrastructure
become blind spots in production.

**Composability model:** The pipeline supports `--stop-after N`, `--phase N`,
`--resume-from N`, `--skip-build`, and `--build-local` flags. These allow
developers to run subsets of phases against an already-running devnet without
re-running the full pipeline. But they never compromise fidelity — `--phase 9`
still waits for real block production. There is no "fast mode" that shortens
block times or skips PoW.

**Ecosystem responsibility:** Quick iteration tools (simulators, mock
networks, RPC-based test harnesses) belong in the ecosystem, not in the
core repository. The core pipeline's job is production validation. If you
need faster feedback, use:
- Level 1 (`cargo test`) for unit and integration tests
- Level 2 (`cargo test --release`) for ZK proof and heavyweight tests
- Python simulations for contract state machine validation
- Python consensus models for block production and VM testing

## Testing Philosophy: The A/B/C Partition

The testing taxonomy (Levels 1–4) describes *scale*. A second axis of
classification — *what the test actually witnesses* — is equally important
and was formalized during the absorber-program MoC review
([type-system.md §10.5](../type-system.md)):

**A. Statically-proven interior.** Facts the compiler or the Lean proof
assistant discharge: nominal type distinctions, `BlockHeight` domain
composition, barb pareto-efficiency, authorization inversion, wallet
construction soundness, and `bridge_safe` quarantine direction.

Tests SHALL NOT re-verify partition-A facts. A test whose failure condition
is "the code failed to compile" SHALL be demoted or removed. The compiler IS
the test for the statically-proven interior. Example of what NOT to test:
`assert_eq!(block.header.height, 42)` when the compiler already enforces
`BlockHeight: PartialEq`.

**B. Absorber boundary.** Runtime enforcement at every quote/eval edge
([type-system.md §10.5](../type-system.md)): the P2P wire, mempool
admission, contract entrypoints (spend-hook re-lift), wallet manifest
parsing, persistence lifts (`from_le_bytes`, SQLite rows), WASM host FFI
(`try_from` edges, ACL, gas), C ABI (null checks, buffer caps), JSON-RPC
(param lifts). Tests in B are **enforcement witnesses** — they verify the
declared barb/budget set holds against an adversary who can send arbitrary
bytes, which phantom types cannot prevent. Every declared SHALL at a boundary
SHALL have at least one runtime witness test. The P2P ban tests are
type-system tests in partition B.

Current boundary obligation coverage (MoC review 2026-08-07, updated
after type-system remediation): 24/36 cells witnessed (67%). Eleven
boundary witness tests (BW-1 through BW-13, with BW-8/9 dispensed)
added coverage across the C FFI, Wallet Manifest, Contract Entrypoints,
Mempool Admission, and Persistence boundaries. The five highest-priority
gaps are documented in type-system.md §10.5.

**C. Dynamic residue.** Genuinely emergent runtime properties: scheduling and
weak bisimilarity (JoinSet merge barrier), network topology and PEX
propagation, economics (fees, emission, supply convergence), timing/finality,
PoW difficulty adjustment, monero merge-mining. Partition C tests depend on
timing, concurrency, adversary behavior, network topology, or economic
equilibrium. The Docker pipeline (Levels 3–4) belongs here. "Testing finds a
balance" between B and C: a boundary that rejects adversaries promptly and
silently passes honest traffic is well-balanced.

### What changes from the old taxonomy

1. Ban tests ARE type-system tests (partition B), not "P2P infrastructure,
   not blockchain/type-system, timing-sensitive" — prior classification
   superseded. CI-gated with `BanPolicy::Strict` + `p2p_local: true` pinned.
2. Full-suite runs are NOT migration gates for compile-proven changes. The
   `BlockHeight` migration's correctness is discharged in A; its residual
   risk — canonical 8-byte sled key width — is a single B witness
   (`test_block_height_persistence_roundtrip` in chain_state.rs).
3. The Docker pipeline is the balance-finding arena (partition C, B
   witnesses collected alongside). Success criterion unchanged: wallet scan
   + decrypt + DRKW balance.

## When to Use Each Level

### Level 1 — Lightweight (Local)

Use when you are:
- Writing or modifying contract model/serialization code
- Adding a new function enum variant
- Testing that contracts deploy correctly through the **Deployooor contract** (the real production path)
- Running fast CI checks (no ZK proof overhead)

**What it covers:** Deployooor-based deployment (DeployV1 → WASM validation →
lock tree → __initialize), data model correctness, serialization round-trips,
type conversions, WASM binary validity.

**What it does NOT cover:** ZK proof generation, contract function behavior,
state transitions with ZK verification, uncle-merkle block execution.

See [Level 1: Lightweight Tests](level-1-lightweight.md).

### Python Contract Simulations — Smoke Test Layer

Use when you are:
- Designing a new contract's state machine or authorization flow
- Adding a new function and want to verify it doesn't create state machine holes
- Testing "what if" scenarios (issuer never pays, coverage drops, timeout expires)
- Checking that every `active` flag has a deactivation path (Safety Principle 3)
- Iterating on business logic without waiting 4-7 minutes per test run

**What it covers:** State machine transitions (legal and illegal), authorization
gates, business rule constraints (collateral ratios, coverage minimums, timeouts),
capability lifecycles, edge cases like double-spend and race conditions.

**What it does NOT cover:** ZK proof generation, WASM execution, cryptographic
operations, network behavior, block production.

All 27 contracts are modeled. See
[Python Contract Simulations](python-simulations.md).

### Python Consensus Models — Pre-Code Specification

Use when you are:
- Modifying block production, PoW validation, or difficulty adjustment
- Fixing consensus bugs (chain splits, target divergence, reorg failures)
- Debugging concurrency issues (RandomX FFI segfaults, VM cache races)
- Adding or changing consensus rules (competing blocks, uncle rewards, finality)
- Verifying that Rust produces identical outputs to the Python specification

**What it covers:** Block production, PoW target computation, difficulty
adjustment, competing block storage with dedup, uncle-merkle consensus,
chain reorganization (Bitcoin ActivateBestChain), atomic reorg validation,
timestamp validation, VM concurrency state machine, finality anchoring
(Caribina + Monero).

**What it does NOT cover:** ZK proofs, WASM execution, P2P networking,
Docker container behavior, async runtime scheduling.

The consensus models are the **authoritative specification** for the Rust
implementation. Every function in `chain_validation_model.py` maps 1:1 to
a function in `src/linear/`. Every scenario in `vm_state_model.py` maps
to a concurrency invariant enforced by the per-VM Mutex.

**Model validated by dockernet**: The Python model's five-node uncle-merkle
predictions (70+ uncle blocks, 300+ competing blocks across 5 full-capacity
miners) were confirmed by the `--nodes 5` consensus pipeline. All 5 nodes
mined at full capacity, P2P mesh held, blocks propagated, competing blocks
became uncles. No segfaults. Pipeline ran 24 minutes, reached heights 17-20
before hitting the limits of a 24-thread/48GB machine.

See [Python Contract Simulations](python-simulations.md) for the consensus
model documentation.

### Resource Requirements by Node Count

| Nodes | Profile | RAM | CPU | Use Case |
|-------|---------|-----|-----|----------|
| 1 | `--nodes 1` (solo) | ~2 GB | 2 threads | Contract dev, tx testing, rapid iteration |
| 2 | `--nodes 2` (native) | ~8 GB | 8 threads | P2P verification, basic block production |
| 5 | `--nodes 5` (consensus) | ~24 GB | 24+ threads | Uncle-merkle consensus verification |
| merge | `--mode merge` | ~32 GB | 28+ threads | Merge mining (2 merge + 1 native + monerod). Offline default (fixed difficulty 1000, no sync needed). Public testnet sync takes ~12h, ~100GB |

The 5-node consensus profile pushed a 24-thread/48GB machine to its limits.
The merge profile adds monerod + p2pool/xmrig sidecars (each node self-contained).
Most developers should use 1 or 2 nodes for daily work. The 5-node and merge
profiles are for consensus confirmation by protocol developers and CI.

### Mining Jitter (Localdev Only)

In production PoW, miners have different effective hash rates and network
propagation delays that cause one miner to pull ahead naturally. In local
testnets with identical Docker containers and short block times (2 seconds),
both miners always find blocks at the same pace, staying on diverged forks
indefinitely. This is a test-environment artifact, not a consensus bug.

The built-in miner task (`miner_task` in `bin/dwowd/src/lib.rs`) adds a
random 0-4 second delay before each mining cycle to break this symmetry.
The faster miner pulls ahead, the slower miner receives blocks before
mining its own, and uncle chain reorg converges the chains. This jitter
is **localdev-only** — production RandomX mining provides natural variance
and does not need it.

If you see "both nodes mining at identical pace" in a local testnet,
verify that `miner_task` includes the jitter delay. Without it, two
nodes with equal resources will never converge because neither chain
grows longer than the other.

See
[Python Contract Simulations](python-simulations.md) for the consensus
model documentation.

### Level 2 — Heavyweight (Local)

Use when you are:
- Writing ZK circuit code or proof generation logic
- Testing full contract business logic with real ZK proofs
- Verifying cross-contract interactions (e.g., recruitment pipeline)
- Testing state transitions that depend on ZK verification
- Testing uncle-merkle block formation (multi-uncle, depth, mixed exec)

**What it covers:** Full ZK proof generation and verification, contract
function execution, state transitions, multi-holder workflows, cross-contract
calls, uncle-merkle block stress (canonical/uncle/mixed/multi-uncle/depth),
gas limit tracking.

**ZK coverage enforcement:** The `HeavyweightPipeline` performs a
pre-deploy ZK coverage check via `verify_zk_coverage()` on every harness.
ZK proof enforcement is structural and immutable (`const STRICT_ZK: bool = true`).
Empty proofs for ZK-gated functions are rejected by the uniform runner's
`submit_block()` using `EndpointSpec::is_zk` (authoritative contract metadata,
never a heuristic). There is no opt-in toggle — all heavyweight tests enforce
STRICT_ZK. A CI audit test at `src/contract/test-harness/tests/zk_audit.rs`
cross-checks every harness's `circuits()` list against the `.zk.bin` files on
disk (175 `.zk.bin` files exist across contract `proof/` directories). Each
harness `spawn()` builds its proving keys, so the audit is ZK-setup-bound
(~80 min), not decode-bound.

**What it does NOT cover:** Deployment correctness — this is tested by
Level 1 through the Deployooor contract. Level 2 uses the direct
`deploy_contract()` path solely for test setup convenience.

**Caveats:** Requires `--release` mode and `RUST_MIN_STACK=67108864` (64MB)
for halo2 proving keys. All 43 tests complete in ~480 seconds.

See [Level 2: Heavyweight Tests](level-2-heavyweight.md).

### Level 3 — Containerized Localnet (Local)

Use when you are:
- Testing P2P networking between multiple nodes
- Verifying block propagation and sync
- Testing mining (RandomX PoW) with realistic parameters
- Running integration scripts that need a live multi-node network
- Testing contract deployment against a running testnet

**What it covers:** 3-container stack (lilith seed + 2 mining nodes), P2P
gossip, block production, RandomX mining via xmrig, RPC endpoint interaction,
bridge lifecycle (8 phases: deploy→init→register→deposit→withdraw→accept→execute→verify),
wallet container with sync/scan/balance/transfer verification, join modes for
public testnet participation, contract E2E testing.

See [Level 3: Containerized Localnet](level-3-localnet.md). The normative gate for
when L3 may run and what a passing run proves is the
[L3 Readiness Specification](l3-readiness-spec.md); the current tree's conformance
is tracked in the [Test Suite Audit](test-audit.md).

### Level 4 — Containerized Devnet (Public)

Use when you are:
- Deploying a node on an idle machine to join a shared devnet
- Running a multi-machine devnet across your LAN
- Opening a devnet to external participants over the internet
- Providing a seed node for others to connect to

**What it covers:** Single-container mining node, host networking for LAN
discovery, env-var-driven configuration, xmrig auto-mining, seed/miner role
selection.

See [Level 4: Containerized Devnet Node](level-4-devnet.md).

## Relationship Between Levels

```
Level 1 (Lightweight)        Fast, no ZK, no P2P, single process
    │
    ├──► Level 1.5 (Bridge)  Production-path integration: real ZK proofs,
    │         real AEAD encryption, real accept_block, real wallet scan.
    │         BlockTarget::MAX, deterministic ZK. ~2-7 min.
    │         Enforces MoC gate — MUST pass before Docker pipeline.
    │
    └──► Level 2 (Heavyweight)  Adds ZK proofs, real execution, still single process
            │
            └──► Level 3 (Localnet)    Adds P2P networking, Docker, multiple nodes
                    │
                    └──► Level 4 (Devnet)     Adds multi-machine, LAN/internet, public access
```

Each level builds on the one before it. Level 1 verifies the code compiles and
serializes correctly. Level 2 verifies ZK proofs and business logic. Level 3
verifies P2P networking works between containers. Level 4 makes it deployable
on real machines across networks.

## AI Safety Connection

The sequential, deterministic nature of this pipeline makes it uniquely
suited for AI-assisted development. AI-generated code can be tested through
all four levels with reproducible results — Uncle Merkle consensus
guarantees the same block always produces the same state, and O-Cap
authorization guarantees AI-written contracts cannot escape their
capability boundaries. When a developer runs the full pipeline with no
gaps, the resulting contract has been verified against compilation,
serialization, ZK proof, multi-node networking, and deployment failure
modes — a broader surface area than most industry smart contract audits
cover.

See [AI-Assisted Development](../ai-assisted-development.md) for the full
philosophy and workflow.

## MoC Test Boundaries

The Management of Change (MoC) review established two distinct test regimes,
separated by what can be tested in Rust unit tests vs. what requires the full
Docker pipeline.

### Pre-Devnet Ceiling (`cargo test`)

Pre-devnet tests operate at the **single-block, single-process** level.
They validate correctness of the consensus-critical code path (genesis,
block creation, and WASM execution) without multi-node networking.

| Ceiling | What's Tested | What's NOT Tested |
|---------|--------------|-------------------|
| **Height 2** | Genesis determinism (AC2-AC9), single block via `accept_block`, cumulative supply bridge (S_2 = S_1 + C_2), hash chain continuity | Multi-block chain growth, competing blocks, uncle resolution |
| **Single node** | `GenesisHarness::new()`, `init_genesis()`, `accept_block()` | P2P networking, multi-node block propagation, sync |
| **No real PoW** | `u32::MAX` target (instant blocks), deterministic ZK (`dwow_native_token_contract::enable_deterministic_zk()`) | Real RandomX mining, xmrig integration, target adjustment |

**Key tests:** `test_genesis_determinism` and `test_block_creation` in
[`bin/dwowd/src/tests/genesis.rs`](../../../bin/dwowd/src/tests/genesis.rs).

**Bridge tests:** `test_wallet_coinbase_scan_only`,
`test_canonical_call_failure_rejects_block`,
`test_merge_mined_block_acceptance`, and
`test_merge_mined_block_deterministic` exercise the exact production code path
(`build_linear_coinbase` → `accept_block` → `scan_block_linear`) at
`BlockTarget::MAX` with deterministic ZK. See "Pre-Production Integration
Tests" above for the full specification.

**Pre-devnet tests enforce the MoC gate:** A contract that fails
`test_block_creation` (height-2 block with PoWRewardV1 coinbase through
`accept_block`) or any bridge test must not proceed to the Docker pipeline.
The pre-devnet tests are the last deterministic, single-process checkpoint
before real networking and real PoW are introduced.

### Docker Pipeline (Multi-Block / Multi-Node)

Everything beyond height 2 belongs in the containerized pipeline
(`contrib/docker/darkwow-testnet/test_pipeline.sh`):

| Capability | Tested By |
|-----------|----------|
| Multi-block chain growth (heights 3+) | `test_pipeline.sh --mode native` |
| Competing block / uncle resolution | `test_pipeline.sh --nodes 5` |
| P2P block propagation and sync | `test_pipeline.sh --mode native` |
| Real RandomX mining (xmrig) | All pipeline modes |
| Bridge lifecycle (8 phases, cross-chain) | `test_pipeline.sh --mode bridge` |
| Merge mining (Monero + p2pool) | `test_pipeline.sh --mode merge` |
| Wallet E2E (scan/balance/transfer) | `test_pipeline.sh --mode wallet` |
| L1 capability write-path (box put/take, wallet-driven) | `test_pipeline.sh --capability-tests` (opt-in, in-docker) |

**Rationale:** The Docker pipeline is production test infrastructure
(Level 3), operating at a 1/8 scale model of the production network.
It is slow by design (20-40 minutes minimum) and uses no mocked components.
A failure in the Docker pipeline is a real consensus bug, not a test
environment artifact.

### GenesisHarness Tests

Two test functions validate the pre-devnet ceiling and enforce the MoC gate:

**`test_genesis_determinism`** — Two independent GenesisHarness setups MUST
produce identical genesis blocks. Acceptance criteria: AC2 (cumulative supply
at height 1 == `INITIAL_REWARD`), AC4 (identical hash), AC5 (total_reward),
AC6 (previous == `[0u8; 32]`), AC7 (timestamp == 0), AC8 (nonce == 0),
AC9 (target == `u32::MAX`).

**`test_block_creation`** — Genesis → build height-2 coinbase-only block
→ submit through `accept_block` (production path, WASM executes).
Acceptance criteria: AC2 (S_1 == INITIAL_REWARD), AC3 (height-2 accepted),
AC4 (hash chain continuous), AC5 (S_2 == S_1 + C_2), reward correctness,
block retrievability.

**Genesis contract initialization** — There is no separate
`init_genesis_contracts()` function. The nine genesis contracts ride inside the genesis
block as deployment transactions (`build_genesis_deployment_txs()` in
`bin/dwowd/src/lib.rs`) and are materialized during genesis-block execution by
`apply_genesis_deployments()` (`src/linear/src/execution.rs`), which deploys each WASM at
its well-known ContractId and invokes `__initialize` (with empty init params — this seeds
ZK circuits, Merkle trees, nullifier roots, and contract state). `GenesisHarness::new()`
additionally stores the 9 WASM binaries directly via `set_contract_data()` as a convenience
for wallet-only tests; the consensus tests use `new_without_contracts()` and rely on the
genesis-deployment rule. `init_genesis()` then creates the genesis block with its
PoWRewardV1 coinbase.

### Pre-Production Integration Tests (the Bridge)

Four tests exercise the **EXACT production code path** — `build_linear_coinbase`
(real ZK proof + AEAD encryption), `accept_block` (full WASM execution),
`scan_block_linear` (wallet production scan), and `capability_balance`. They
use `BlockTarget::MAX` (instant PoW) and deterministic ZK for reproducibility.
They are the last deterministic, single-process checkpoint before the Docker
pipeline introduces real networking and real RandomX.

**`test_wallet_coinbase_scan_only`** (`wallet_integration.rs`) — Production
coinbase → wallet scan → decryption → DRKW balance. Exercises the full
ρ-calculus coinbase lifecycle: ↓mine (coinbase commitment), ↓encrypt (AEAD
note), ↓verify (ZK proof), ↓commit (WASM state write), ↓discover (wallet
trial decryption), ↓denominate (DRKW asset identification), ↓derive
(per-block key derivation). Both genesis and post-genesis blocks use
`build_linear_coinbase` + `accept_block`. DH commutativity validated:
`sapling_ka_agree(sk_H, epk) == sapling_ka_agree(esk, pk_H)`.

**`test_canonical_call_failure_rejects_block`** (`wallet_integration.rs`) —
Gap 14 closure. A canonical call to a non-existent contract MUST reject the
entire block (strict mode, `execution.rs:408-411`). Chain height MUST NOT
advance. This is the primary defense against partially-applied state from
mixed success/failure blocks.

**`test_merge_mined_block_acceptance`** (`merge_mining.rs`) — Merge-mined
block with real `build_linear_coinbase` coinbase through `accept_block`.
Verifies Monero-merge-mining blocks enter the standard acceptance path
(`PowSource::Monero`), skip native RandomX PoW, and are stored with real
nullifier tracking.

**`test_merge_mined_block_deterministic`** (`merge_mining.rs`) — Two
independent harnesses with real coinbases produce identical merge-mined
block hashes. Validates the determinism invariant: same MoneroPowData +
same chain state + same key → same block hash.

**Write-path coverage:** these four bridge tests cover the *coinbase receive* path. The
wallet's **write path** (transfer/spend) and the **capability transfer path** (Box/PromissoryNote/
Purse put/take) are covered by the [L1 Capability Write-Path
Specification](l1-capability-write-path-spec.md) — the authoritative real-vs-fake coverage matrix. The
**harness-driven** acceptance tests (Box put/take, PN transfer/redeem, Purse deposit/withdraw) reach each
contract's `*_roots` gate through `accept_block` but build proofs with the test harness; the native
`test_transfer_accepts_through_accept_block` spends DRKW through `build_native_transfer`. The wallet's
**generic-prover** write path (production fidelity) is on-chain-witnessed for **box put**
(`test_box_put_wallet_driven_generic_prover`), **box take** (`test_box_take_wallet_driven_generic_prover`),
and **purse deposit/withdraw** (`test_purse_deposit_withdraw_wallet_driven_generic_prover`);
box transfer-to-new-owner is note-only (not on-chain). Wallet-driven PN remains an open gap (see the
spec §4.2). F-6 (harness-driven coverage) is CLOSED; the wallet-driven fidelity gap is tracked as F-15 in
the [Test Suite Audit](test-audit.md).

Command:
```
RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 \
  cargo test --release -p dwowd --lib -- \
  test_wallet_coinbase_scan_only \
  test_canonical_call_failure_rejects_block \
  test_merge_mined_block_acceptance \
  test_merge_mined_block_deterministic \
  --nocapture
```

These tests SHALL pass before any code proceeds to the Docker pipeline.
They enforce the MoC gate: a contract or consensus change that breaks
these tests has introduced a regression in the production coinbase, WASM
execution, wallet scan, merge-mining, or strict-mode rejection paths.

## File Map

| Component | Path |
|-----------|------|
| Contract unit/integration tests | `src/contract/<name>/tests/` — 31 contracts have `integration.rs`; `native_token` has `unit.rs`; `entropy` is a library |
| Contract ZK circuit tests | `src/contract/<name>/tests/zk_circuit_test.sh` — all 31 ZK-enabled contracts |
| Test harness crate (32 contracts) | `src/contract/test-harness/` |
| ZK coverage CI audit test | `src/contract/test-harness/tests/zk_audit.rs` |
| Encode/decode round-trip test | `src/contract/test-harness/tests/encode_roundtrip.rs` |
| Relayer unit tests (Level 1) | `bin/universal_relayer/src/` |
| Relayer lightweight test runner | `bin/universal_relayer/test_relayer_lightweight.sh` |
| Daemon integration tests | `bin/dwowd/src/tests/` |
| **Pre-production bridge tests** | `bin/dwowd/src/tests/wallet_integration.rs` (T3, canonical call failure), `bin/dwowd/src/tests/merge_mining.rs` (merge-mined block acceptance + determinism) |
| **Wallet transfer/spend acceptance (Level 1.5)** | `bin/dwowd/src/tests/wallet_transfer_integration.rs` (`test_transfer_accepts_through_accept_block`) |
| **Capability transfer acceptance (Level 1.5)** | `bin/dwowd/src/tests/capability_scan_integration.rs` — harness-driven Box put/take, PN transfer/redeem, Purse deposit/withdraw + wallet-driven generic-prover Box put (`test_box_put_wallet_driven_generic_prover`), Box take (`test_box_take_wallet_driven_generic_prover`), Purse deposit/withdraw (`test_purse_deposit_withdraw_wallet_driven_generic_prover`), and Box transfer-to-new-owner (`test_box_transfer_to_new_owner_wallet_driven`); wallet-driven PN remains an open gap (spec §4.2, audit F-15) |
| Genesis determinism + sync tests | `bin/dwowd/src/tests/genesis.rs` |
| Lightweight deployment tests | `bin/dwowd/src/tests/pipeline.rs` |
| Block execution tests (Level 2) | `bin/dwowd/src/tests/heavyweight_pipeline.rs` |
| Fee collect determinism (Level 2) | `bin/dwowd/src/tests/fee_collect_pipeline.rs` |
| Consensus coordination tests | `bin/dwowd/tests/consensus_coordination.rs` |
| Tripwire guardrails | `bin/dwowd/src/tests/tripwire.rs` |
| Boundary witness: Mempool admission (BW-2) | `src/linear/src/zk_verifier.rs` (`test_native_token_proofless_call_rejected_at_admission`) |
| Boundary witness: ContractId rejection (BW-3) | `src/sdk/src/crypto/contract_id.rs` (`test_contract_id_rejects_wrong_length`) |
| Boundary witness: Entrypoint data gating (BW-4) | `src/contract/native_token/tests/unit.rs` (`test_entrypoint_data_length_gating`) |
| Boundary witness: Manifest caps (BW-5/6) | `src/sdk/src/manifest.rs` (`test_field_count_caps_enforced`, `test_witness_binding_depth_rejected`) |
| Boundary witness: WalletDB persistence (BW-7) | `bin/dww/src/scan.rs` (`test_walletdb_persistence_roundtrip`) |
| Boundary witness: C FFI safety (BW-10/11/12) | `bin/dww/src/ffi.rs` (`test_ffi_null_pointers_rejected`, `test_ffi_buffer_caps_enforced`, `test_ffi_catch_unwind_isolation`) |
| Boundary witness: Nullifier replay (BW-1) | `src/linear/src/chain_state.rs` (`test_nullifier_replay_detected`, `#[ignore]` — pending API) |
| Boundary witness: Supply persistence (BW-13) | `src/linear/src/chain_state.rs` (`test_supply_chain_persistence_roundtrip`) |
| Relayer heavyweight test runner | `bin/dwowd/src/tests/heavyweight.sh --relayer` |
| Docker base image (shared by all images) | `contrib/docker/darkwow-testnet/Dockerfile.base` |
| Docker localnet (modular pipeline) | `contrib/docker/darkwow-testnet/` (18 `lib/*.sh` modules + `test_pipeline.sh` orchestrator + `pipeline_spec.py` spec) |
| Bridge Docker image + pipeline (Level 3) | `contrib/docker/bridge-node/` |
| Wallet lightweight test runner (Level 1) | `bin/dww/test_capability_lightweight.sh` |
| Wallet capability tests (Level 2) | `bin/dww/src/capability.rs` |
| Wallet Docker image, entrypoint (Level 3) | `contrib/docker/darkwow-testnet/Dockerfile.wallet`, `contrib/docker/darkwow-testnet/entrypoint-wallet.sh` |
| Wallet container test script (Level 3) | `contrib/docker/darkwow-testnet/test-wallet.sh` |
| Docker devnet (3-container, fast iteration) | `contrib/docker/darkwow-devnet/` |
| Docker devnet node (single-container) | `contrib/docker/darkwow-devnet/` |
| Manual localnet scripts | `contrib/localnet/` |
| Public testnet management scripts | `contrib/testnet/` |

### Memory Rules (AI-Assisted Development)

The [AI-Assisted Development](../ai-assisted-development.md) workflow
references several "memory rules" that guide safe tool usage:

| Rule | Summary |
|------|---------|
| GenesisHarness is test-only | Never use GenesisHarness in production code. It uses `u32::MAX` target and temp sled DBs. |
| Pre-devnet ceiling at height 2 | Unit tests stop at `test_block_creation`. Multi-block, multi-node tests belong in the Docker pipeline. |
| Stale contract WASM | Rebuild WASM before `cargo test` when contracts change. `include_bytes!` embeds at compile time. |
| Pipeline is step 7 of 7 | "Code compiles" does not mean "run the pipeline." Verify locally first. |
| Never poll a running pipeline | Start in background; wait for harness notification. |
| AccountManager is the key authority | No shell-level key manipulation. Use `import_base58`/`export_base58` for key sharing. |
| Genesis contracts ride in the genesis block | Deployments materialize via `apply_genesis_deployments()` during genesis-block execution; then `init_genesis()` creates the genesis block. |
| Single `accept_block` path | All 5 mining entry points route through `accept_block()` in `block_acceptor.rs`. No dual paths. |
