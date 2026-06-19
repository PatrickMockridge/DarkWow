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
| 1 | Lightweight | Unit tests, integration tests, **Deployooor-based deployment** (real production path, no ZK) | Seconds | `cargo test`, `cargo test -p dwowd test_pipeline` |
| — | **Python Simulations** | **Contract state machines, authorization flows, business rules, edge cases** (no ZK, no crypto — pure logic) | Milliseconds | `python3 -c "from sim.contracts..."` |
| — | **Python Consensus Models** | **Block production, PoW, uncle-merkle, chain reorg, VM concurrency, finality, merge mining** (1:1 Rust specification, 34/34 tests, 8 VM scenarios) | Milliseconds | `python3 contrib/model/chain_validation_model.py`, `python3 contrib/model/vm_state_model.py`, `python3 contrib/model/merge_mining_model.py` |
| 2 | Heavyweight | Contract **functions, ZK proofs, uncle-merkle block execution** (deployment not tested — uses direct path for setup) | Minutes | `cargo test --release -p dwowd test_<contract>_heavyweight` |
| 3 | Containerized Localnet | Multi-node Docker testnet (seed + mining nodes), P2P, RandomX mining | Persistent | `docker-compose up` in `contrib/docker/darkwow-testnet/` |
| 4 | Containerized Devnet | Public-facing mining node for shared devnets over LAN/internet | Persistent | `docker run --network=host -e IS_SEED=true darkwow-devnet` |
| Wallet | Wallet capabilities | L1: Bash CLI (seconds). L2: Rust in-process (20 tests, <2s). L3: Docker container (persistent). | Seconds to Persistent | `./bin/drk/test_capability_lightweight.sh`, `cargo test -p dwow_wallet --lib -- capability::tests`, `./contrib/docker/darkwow-testnet/test-wallet.sh` |
| Wallet | Wallet in Dockernet | End-to-end wallet testing with mining nodes + wallet container. Guardrailed commands, verified subcommand syntax, pre-flight checklist. | Persistent | See [Wallet Testing in Dockernet](wallet-testing.md) |

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
The `strict_zk` mode (opt-in) rejects empty proofs for ZK contracts instead
of just warning. A CI audit test at `src/contract/test-harness/tests/zk_audit.rs`
decodes all 99 harness-loaded `.zk.bin` files in under a second (no proving
key building) and cross-checks harness `circuits()` lists against zkbin
files on disk.

**What it does NOT cover:** Deployment correctness — this is tested by
Level 1 through the Deployooor contract. Level 2 uses the direct
`deploy_contract()` path solely for test setup convenience.

**Caveats:** Requires `--release` mode and `RUST_MIN_STACK=67108864` (64MB)
for halo2 proving keys. All 36 tests complete in ~450 seconds.

See [Level 2: Heavyweight Tests](level-2-heavyweight.md).

### Level 3 — Containerized Localnet (Local)

Use when you are:
- Testing P2P networking between multiple nodes
- Verifying block propagation and sync
- Testing mining (RandomX PoW) with realistic parameters
- Running integration scripts that need a live multi-node network
- Testing contract deployment against a running testnet

**What it covers:** 3-container stack (lilith seed + 2 mining nodes), P2P
gossip, block production, RandomX mining via xmrig, RPC endpoint interaction.

See [Level 3: Containerized Localnet](level-3-localnet.md).

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

## File Map

| Component | Path |
|-----------|------|
| Contract unit/integration tests | `src/contract/<name>/tests/` |
| Test harness crate (27 contracts) | `src/contract/test-harness/` |
| ZK coverage CI audit test | `src/contract/test-harness/tests/zk_audit.rs` |
| Relayer unit tests (Level 1) | `bin/universal_relayer/src/` |
| Relayer lightweight test runner | `bin/universal_relayer/test_relayer_lightweight.sh` |
| Daemon integration tests | `bin/dwowd/src/tests/` |
| Bridge lifecycle test (Level 2) | `bin/dwowd/src/tests/heavyweight_pipeline.rs` |
| Relayer heavyweight test runner | `bin/universal_relayer/test_relayer_heavyweight.sh` |
| Docker base image (shared by all images) | `contrib/docker/darkwow-testnet/Dockerfile.base` |
| Docker localnet (3-container) | `contrib/docker/darkwow-testnet/` |
| Bridge Docker image + pipeline (Level 3) | `contrib/docker/bridge-node/` |
| Wallet lightweight test runner (Level 1) | `bin/drk/test_capability_lightweight.sh` |
| Wallet capability tests (Level 2) | `bin/drk/src/capability.rs` |
| Wallet Docker image, entrypoint (Level 3) | `contrib/docker/darkwow-testnet/Dockerfile.wallet`, `contrib/docker/darkwow-testnet/entrypoint-wallet.sh` |
| Wallet container test script (Level 3) | `contrib/docker/darkwow-testnet/test-wallet.sh` |
| Docker devnet (3-container, fast iteration) | `contrib/docker/darkwow-devnet/` |
| Docker devnet node (single-container) | `contrib/docker/darkwow-devnet/` |
| Manual localnet scripts | `contrib/localnet/` |
| Public testnet management scripts | `contrib/testnet/` |
