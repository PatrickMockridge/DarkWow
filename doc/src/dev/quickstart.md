# Developer Quick Start Guide

DarkWow ships with a **four-level testing infrastructure** that upstream DarkFi
does not have. Every developer workflow — from fast code iteration to multi-machine
devnet deployment — has a dedicated, documented path. This guide answers: *"I want
to do X — what do I run?"*

The extended smart contract suite (32 contracts with ZK circuits, test harnesses,
and integration tests) is designed to be **forked, customized, and built on**. The
testing infrastructure is the platform that makes this possible.

## Quick Reference

| I want to... | Level | Command | Time |
|---|---|---|---|
| Test contract model/serialization code | 1 | `cargo test -p dwow_<contract> --test integration` | seconds |
| Deploy all contracts (no ZK proofs) | 1 | `cargo test -p dwowd test_pipeline -- --nocapture` | ~2 min |
| Run full contract execution with ZK proofs | 2 | `cargo test --release -p dwowd test_<contract>_heavyweight` | 30-120s |
| Run the test pipeline (4 modes) | 3 | `./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native` | ~5 min |
| Run a multi-node blockchain locally | 3 | `docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml --profile native up -d` | persistent |
| Join or create a shared devnet | 4 | `docker run --network=host -e IS_SEED=true darkwow-devnet` | persistent |
| Join the public testnet as a miner | 4 | `docker run --network=host -e ROLE=dwowd darkwow-testnet:latest` | persistent |
| Run native mining + deploy contracts | N/A | `./contrib/docker/darkwow-testnet/join-testnet.sh --mode native` | ~10 min |
| Run the full contract test suite | 3 | `./contrib/docker/darkwow-testnet/test-contracts.sh` | ~5 min |
| Run a bridge relayer with endowment | 4 | `docker run --network=host -e MODE=full darkwow-bridge-node:latest` | persistent |

## The Four Testing Levels

DarkWow ships with a four-level testing infrastructure. See the [Testing Overview](testing/overview.md) for the complete taxonomy, per-level detail pages, and when to use each level.

| I want to... | Level | Command | Time |
|---|---|---|---|
| Test contract model/serialization code | 1 | `cargo test -p dwow_<contract> --test integration` | seconds |
| Deploy all contracts (no ZK proofs) | 1 | `cargo test -p dwowd test_pipeline -- --nocapture` | ~2 min |
| Run full contract execution with ZK proofs | 2 | `cargo test --release -p dwowd test_<contract>_heavyweight` | 30-120s |
| Run the test pipeline (4 modes) | 3 | `./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native` | ~5 min |
| Run a multi-node blockchain locally | 3 | `docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml --profile native up -d` | persistent |
| Join or create a shared devnet | 4 | `docker run --network=host -e IS_SEED=true darkwow-devnet` | persistent |
| Join the public testnet as a miner | 4 | `docker run --network=host -e ROLE=dwowd darkwow-testnet:latest` | persistent |
| Run native mining + deploy contracts | N/A | `./contrib/docker/darkwow-testnet/join-testnet.sh --mode native` | ~10 min |
| Run the full contract test suite | 3 | `./contrib/docker/darkwow-testnet/test-contracts.sh` | ~5 min |
| Run a bridge relayer with endowment | 4 | `docker run --network=host -e MODE=full darkwow-bridge-node:latest` | persistent |

### Level 3 — Containerized Localnet (test_pipeline.sh)

The test pipeline supports four modes — two local devnet modes plus two join modes for connecting to the public testnet as an external participant. Each mode builds Docker images, starts the stack, and runs 10-12 sequential verification phases (clean, prereqs, wallet, build, start, container verification, RPC health, mining activity, block production, report, plus persistence and seed fallback for join modes).

```bash
# Full pipeline — 4 modes (clean → build → verify)
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode merge
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode join-native
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode join-merge
```

See the [darkwow-testnet README] for the full modes comparison table, Docker image catalog, and compose profile reference.

[darkwow-testnet README]: https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/contrib/docker/darkwow-testnet/README.md

→ [Level 3: Containerized Localnet](testing/level-3-localnet.md)

### Level 4 — Containerized Devnet (LAN/Internet, Persistent)

A single-container mining node for **multi-machine shared devnets**. Deploy a seed on one machine, miners on others — all connected over LAN or the public internet.

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

→ [Level 4: Containerized Devnet Node](testing/level-4-devnet.md)

### Public Testnet Node (Docker Hub)

The `darkwow-testnet` image serves as a single-container mining node for joining
the **public DarkWow testnet**. The image runs the node (dwowd); the wallet
(`dwow_wallet`) runs natively on the host.

```bash
docker pull darkwow-testnet:latest

# Native RandomX mining (solo)
docker run -d --name dwow-node --network=host \
    -e ROLE=dwowd \
    -e WALLET_ADDRESS="<bs58-address>" \
    -e WALLET_SECRET_FILE=/run/secrets/mining_secret \
    -e SEED_ADDR=lilith0.dark.fi:31340,lilith1.dark.fi:31340 \
    -e MAGIC_BYTES=68,82,75,87 \
    -v /data/dwowd:/root/.local/share/dwow/dwowd \
    -v /path/to/secret:/run/secrets/mining_secret:ro \
    darkwow-testnet:latest

# Merge mining (Monero testnet + DarkWow via p2pool) — use join-testnet.sh
./contrib/docker/darkwow-testnet/join-testnet.sh --mode merge
```

→ [darkwow-testnet README](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/contrib/docker/darkwow-testnet/README.md)

## Contract Suite

DarkWow includes **32 smart contracts** across six domains. Every contract ships
with ZK circuits, a test harness implementing the `ContractHarness` trait, and
integration tests at both Level 1 and Level 2.

| Domain | Contracts | Harnesses |
|---|---|---|
| **Finance** | native_token, promissory_note, stablecoin, dex, bridge, pool_stake | ✅ All |
| **Gaming** | baccarat, roulette, slot, darktoshi_dice, lottery, game_room, betting_stake | ✅ All |
| **Governance** | dao_escrow, subscription, labor_market, tender | ✅ All |
| **Identity** | identity, attestation, oracle | ✅ All |
| **Exchange** | auction, escrow, darkbet_exchange | ✅ All |
| **Infrastructure** | deployooor, drain_protection, insurance_market, relayer_endowment | ✅ All |

Each harness loads the contract's ZK circuits at compile time and provides typed
methods for building and proving contract calls — no blockchain required for
Level 1 and 2 testing.

```rust
// Example: testing a promissory_note mint with the harness
let harness = PromissoryNoteHarness::new()?;
let asset_id = harness.create_token("TestToken")?;
let proof = harness.mint(asset_id, &recipient, 1000)?;
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
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml --profile native up -d

# Or run the full test pipeline (clean → build → verify)
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native
```

**Finality flags for merge mining.** When testing with Monero merge mining,
enable the Monero finality anchor with `--finality-enable-monero` and
optionally configure a monerod RPC endpoint for full anchor verification:

```bash
dwowd --network darkwow-testnet --finality-enable-monero \
    --monerod-rpc-url http://127.0.0.1:18081/json_rpc
```

See [Monero Integration](../arch/monero.md) for the full dual-finality
architecture and [Caribina Finality](../arch/caribina.md) for the
Arweave anchoring layer.

Every contract is self-contained in `src/contract/<name>/` with its own `proof/`
directory (ZK circuits), `tests/` directory (integration tests), and a harness
module in `src/contract/test-harness/src/harness/`. Copy a contract directory as
a template, modify the circuits and entrypoint, register the harness — you have
the full pipeline.

→ [Contributing & Developer Guide](contrib/contrib.md)
→ [Architecture Overview](../arch/overview.md)

## Next Steps

- [AI-Assisted Development](ai-assisted-development.md) — Why DarkWow is safe for AI-generated code and how to use the test pipeline as a vibe-coding safety net
- [Testing Overview](testing/overview.md) — Full four-level taxonomy with file map
- [Level 1: Lightweight Tests](testing/level-1-lightweight.md) — GenesisHarness, ContractTestingPipeline, debug tips
- [Level 2: Heavyweight Tests](testing/level-2-heavyweight.md) — ContractHarness trait, proving keys, cross-contract composition
- [Level 3: Containerized Localnet](testing/level-3-localnet.md) — Docker architecture, wallet setup, contract test scripts
- [Level 4: Containerized Devnet Node](testing/level-4-devnet.md) — Multi-machine deployment, env var reference, internet-facing setup
- [Contract Development Guide](contracts.md) — Smart contract architecture and patterns
- [Contract Standards](contracts/standards.md) — ZK circuit rules, token layer architecture, testing standards
- [Architecture Overview](../arch/overview.md) — Consensus, WASM runtime, token architecture
- [Native Mining + Contract Workflow](native-workflow.md) — Run a node, mine commitments, deploy contracts
- [Bridge Node](bridge-node.md) — Cross-chain bridge relayer with capital endowment
- [ZK Circuit Troubleshooting](zk-circuit-troubleshooting.md) — Debugging circuit issues
