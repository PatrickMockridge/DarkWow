# Developer Quick Start Guide

DarkWow ships with a **four-level testing infrastructure** that upstream DarkFi
does not have. Every developer workflow — from fast code iteration to multi-machine
devnet deployment — has a dedicated, documented path. This guide answers: *"I want
to do X — what do I run?"*

The extended smart contract suite (28+ contracts with ZK circuits, test harnesses,
and integration tests) is designed to be **forked, customized, and built on**. The
testing infrastructure is the platform that makes this possible.

## Quick Reference

| I want to... | Level | Command | Time |
|---|---|---|---|
| Test contract model/serialization code | 1 | `cargo test -p dwow_<contract> --test integration` | seconds |
| Deploy all contracts (no ZK proofs) | 1 | `cargo test -p dwowd test_pipeline -- --nocapture` | ~2 min |
| Run full contract execution with ZK proofs | 2 | `cargo test --release -p dwowd test_<contract>_heavyweight` | 30-120s |
| Run a multi-node blockchain locally | 3 | `docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml up -d` | persistent |
| Join or create a shared devnet | 4 | `docker run --network=host -e IS_SEED=true dwow-devnet` | persistent |
| Run the full contract test suite | 3 | `./contrib/docker/darkwow-testnet/test-contracts.sh` | ~5 min |

## The Four Testing Levels

### Level 1 — Lightweight (Local, Seconds)

Fast unit and integration tests with **no ZK proof overhead**. Tests model code,
serialization, WASM binary validity, and deployment plumbing.

```bash
# Run a single contract's integration tests
cargo test -p dwow_money_v3_contract --test integration

# Run all lightweight contract deployment tests
cargo test -p dwowd test_pipeline -- --nocapture
```

**What it covers:** Data model correctness, serialization round-trips, type
conversions, contract deployment without proofs.

**What it doesn't cover:** ZK proof generation, business logic under real proofs,
P2P networking.

→ [Level 1: Lightweight Tests](testing/level-1-lightweight.md)

### Level 2 — Heavyweight (Local, Minutes)

Full ZK proof generation and contract execution. Exercises real proving keys,
state transitions, and cross-contract calls. Requires `--release` mode or
increased stack size.

```bash
# Single contract heavyweight test
RAYON_NUM_THREADS=10 cargo test --release -p dwowd \
    test_money_v3_heavyweight -- --nocapture

# All heavyweight tests (28+ contracts)
RAYON_NUM_THREADS=10 cargo test --release -p dwowd \
    test_heavyweight -- --nocapture
```

**What it covers:** Full ZK proof generation and verification, contract execution,
state transitions, cross-contract composition, multi-holder workflows.

→ [Level 2: Heavyweight Tests](testing/level-2-heavyweight.md)

### Level 3 — Containerized Localnet (Local, Persistent)

A 3-container Docker stack: **lilith** (P2P seed) + **node0** and **node1**
(mining fullnodes with xmrig). Tests P2P networking, block propagation, and
RandomX mining with realistic parameters.

```bash
# Start the testnet
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml up -d

# Check RPC health
curl -s http://localhost:31345 -X POST \
    -H 'Content-Type: application/json' \
    -d '{"method":"blockchain.info","params":[],"id":1}'

# Run contract deploy + transfer test
./contrib/docker/darkwow-testnet/contract_test.sh

# Run full multi-contract test suite
./contrib/docker/darkwow-testnet/test-contracts.sh

# Tear down
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml down -v
```

**What it covers:** 3-container stack (seed + 2 miners), P2P gossip, block
production, RandomX mining via xmrig, RPC endpoint interaction.

→ [Level 3: Containerized Localnet](testing/level-3-localnet.md)

### Level 4 — Containerized Devnet (LAN/Internet, Persistent)

A single-container mining node for **multi-machine shared devnets**. Deploy a seed
on one machine, miners on others — all connected over LAN or the public internet.

```bash
# Seed machine
docker run --rm --network=host \
    -e ROLE=lilith \
    -e NETWORK=my-devnet \
    -e P2P_PORT=31340 \
    -e MAGIC_BYTES=68,82,75,87 \
    darkwow-testnet:latest

# Miner machine (replace <seed-ip> with actual LAN IP)
docker run --rm --network=host \
    -e ROLE=dwowd \
    -e NETWORK=my-devnet \
    -e P2P_PORT=31342 \
    -e RPC_PORT=31345 \
    -e STRATUM_PORT=31347 \
    -e SEED_ADDR=<seed-ip>:31340 \
    -e MAGIC_BYTES=68,82,75,87 \
    -e MINING_THREADS=4 \
    darkwow-testnet:latest
```

**What it covers:** Multi-machine P2P, host networking for LAN discovery,
env-var-driven configuration, xmrig auto-mining, seed/miner role selection.

→ [Level 4: Containerized Devnet Node](testing/level-4-devnet.md)
→ [darkwow-testnet README](https://github.com/darkrenaissance/darkfi/blob/master/contrib/docker/darkwow-testnet/README.md)

## Contract Suite

DarkWow includes **28+ smart contracts** across six domains. Every contract ships
with ZK circuits, a test harness implementing the `ContractHarness` trait, and
integration tests at both Level 1 and Level 2.

| Domain | Contracts | Harnesses |
|---|---|---|
| **Finance** | native_token, money_v3, stablecoin, dex, bridge, atomic_swap, pool_stake | ✅ All |
| **Gaming** | baccarat, roulette, slot, darktoshi_dice, lottery, game_room, betting_stake | ✅ All |
| **Governance** | dao_escrow, subscription, labor_market, tender | ✅ All |
| **Identity** | identity, attestation, oracle | ✅ All |
| **Exchange** | auction, escrow, darkbet_exchange | ✅ All |
| **Infrastructure** | deployooor, drain_protection, insurance_market, relayer_endowment | ✅ All |

Each harness loads the contract's ZK circuits at compile time and provides typed
methods for building and proving contract calls — no blockchain required for
Level 1 and 2 testing.

```rust
// Example: testing a money_v3 mint with the harness
let harness = MoneyV3Harness::new()?;
let token_id = harness.create_token("TestToken")?;
let proof = harness.mint(token_id, &recipient, 1000)?;
assert!(harness.verify(&proof));
```

→ [Contract Development Guide](contracts.md)
→ [Contract Standards](contracts/standards.md)

## Fork and Build

The contracts and testing infrastructure are **the platform**. Fork the repo,
customize contracts to suit your needs, run the test pipeline to validate, deploy
to a localnet, then scale to a shared devnet.

```bash
git clone https://codeberg.org/PatrickM123/darkwow
cd darkwow
make                                    # Build everything

# Fast iteration: modify a contract, then
cargo test -p dwowd test_pipeline       # Level 1: deploy check

# Validate ZK proofs
RAYON_NUM_THREADS=10 cargo test --release -p dwowd \
    test_<contract>_heavyweight         # Level 2: full ZK

# Spin up a local network
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml up -d
```

Every contract is self-contained in `src/contract/<name>/` with its own `proof/`
directory (ZK circuits), `tests/` directory (integration tests), and a harness
module in `src/contract/test-harness/src/harness/`. Copy a contract directory as
a template, modify the circuits and entrypoint, register the harness — you have
the full pipeline.

→ [Contributing & Developer Guide](contrib/contrib.md)
→ [Architecture Overview](../arch/overview.md)

## Next Steps

- [Testing Overview](testing/overview.md) — Full four-level taxonomy with file map
- [Level 1: Lightweight Tests](testing/level-1-lightweight.md) — GenesisHarness, ContractTestingPipeline, debug tips
- [Level 2: Heavyweight Tests](testing/level-2-heavyweight.md) — ContractHarness trait, proving keys, cross-contract composition
- [Level 3: Containerized Localnet](testing/level-3-localnet.md) — Docker architecture, wallet setup, contract test scripts
- [Level 4: Containerized Devnet Node](testing/level-4-devnet.md) — Multi-machine deployment, env var reference, internet-facing setup
- [Contract Development Guide](contracts.md) — Smart contract architecture and patterns
- [Contract Standards](contracts/standards.md) — ZK circuit rules, token layer architecture, testing standards
- [Architecture Overview](../arch/overview.md) — Consensus, WASM runtime, token architecture
- [ZK Circuit Troubleshooting](zk-circuit-troubleshooting.md) — Debugging circuit issues
