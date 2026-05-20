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
| 1 | Lightweight | Unit tests, integration tests, contract deployment (no ZK proofs) | Seconds | `cargo test`, `cargo test -p dwowd test_pipeline` |
| 2 | Heavyweight | ZK proof generation, full harness, RPC endpoints | Minutes | `cargo test --release -p dwowd test_<contract>_heavyweight` |
| 3 | Containerized Localnet | Multi-node Docker testnet (seed + mining nodes), P2P, RandomX mining | Persistent | `docker-compose up` in `contrib/docker/darkwow-testnet/` |
| 4 | Containerized Devnet | Public-facing mining node for shared devnets over LAN/internet | Persistent | `docker run --network=host -e IS_SEED=true dwow-devnet` |

## When to Use Each Level

### Level 1 — Lightweight (Local)

Use when you are:
- Writing or modifying contract model/serialization code
- Adding a new function enum variant
- Checking that a contract deploys without errors
- Running fast CI checks (no ZK proof overhead)

**What it covers:** Data model correctness, serialization round-trips, type
conversions, WASM binary validity, deployment plumbing.

**What it does NOT cover:** ZK proof generation, business logic under real
proofs, state transitions with ZK verification.

See [Level 1: Lightweight Tests](level-1-lightweight.md).

### Level 2 — Heavyweight (Local)

Use when you are:
- Writing ZK circuit code or proof generation logic
- Testing full contract business logic with real ZK proofs
- Verifying cross-contract interactions (e.g., DEX calling MoneyV3)
- Testing state transitions that depend on ZK verification

**What it covers:** Full ZK proof generation and verification, contract
execution, state transitions, multi-holder workflows, cross-contract calls.

**Caveats:** Requires `--release` mode or increased stack size (halo2 proving
keys are computationally intensive). Each heavyweight test takes 30-120 seconds.

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

## File Map

| Component | Path |
|-----------|------|
| Contract unit/integration tests | `src/contract/<name>/tests/` |
| Test harness crate (28 contracts) | `src/contract/test-harness/` |
| Relayer unit tests (Level 1) | `bin/universal_relayer/src/` |
| Relayer lightweight test runner | `bin/universal_relayer/test_relayer_lightweight.sh` |
| Daemon integration tests | `bin/dwowd/src/tests/` |
| Bridge lifecycle test (Level 2) | `bin/dwowd/src/tests/heavyweight_pipeline.rs` |
| Relayer heavyweight test runner | `bin/universal_relayer/test_relayer_heavyweight.sh` |
| Docker base image (shared by all images) | `contrib/docker/darkwow-testnet/Dockerfile.base` |
| Docker localnet (3-container) | `contrib/docker/darkwow-testnet/` |
| Bridge Docker image + pipeline (Level 3) | `contrib/docker/bridge-node/` |
| Docker devnet (3-container, fast iteration) | `contrib/docker/dwow-devnet/` |
| Docker devnet node (single-container) | `contrib/docker/dwow-devnet/` |
| Manual localnet scripts | `contrib/localnet/` |
| Public testnet management scripts | `contrib/testnet/` |
