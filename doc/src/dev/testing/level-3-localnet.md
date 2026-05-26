# Level 3: Containerized Localnet

A multi-container Docker testnet that mirrors public testnet conditions. Runs
on a single machine with 3 containers: a seed node (lilith) and 2 mining nodes
(dwowd + xmrig).

**Location:** `contrib/docker/darkwow-testnet/`

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│ dwow-lilith  │     │ dwow-node0   │     │ dwow-node1   │
│ (seed)       │◄───►│ (miner)      │◄───►│ (miner)      │
│              │     │              │     │              │
│ P2P: 31340   │     │ P2P: 31342   │     │ P2P: 31343   │
│              │     │ RPC: 31345   │     │ RPC: 31346   │
│              │     │ Stratum:31347│     │ Stratum:31349│
└──────────────┘     └──────────────┘     └──────────────┘
       │                     │                     │
       └─────────────────────┴─────────────────────┘
                  Bridge network: dwow-local
```

| Container | Role | P2P Port | RPC Port | Stratum Port |
|-----------|------|----------|----------|--------------|
| `dwow-lilith` | Seed node | 31340 | — | — |
| `dwow-node0` | Mining node | 31342 | 31345 | 31347 |
| `dwow-node1` | Mining node | 31343 | 31346 | 31349 |

Each mining node runs dwowd + xmrig. Nodes connect to lilith as their seed and
each other as peers. xmrig mines via the local stratum server, with coinbase
rewards paid to an auto-generated mining address.

## Network Parameters

| Parameter | Value |
|-----------|-------|
| Block time | 120 seconds |
| Initial difficulty | 255 (auto-adjusting) |
| PoW algorithm | RandomX (rx/0) |
| Consensus threshold | 3 |
| Magic bytes | `[68, 82, 75, 87]` ("DRKW") |
| `localnet` | `false` (full TLS cert validation) |
| `skip_fees` | `false` |

## Quick Start

```bash
cd contrib/docker/darkwow-testnet

# Full pipeline — 4 modes (clean → build → verify)
./test_pipeline.sh --mode native        # 3-node local devnet, native mining
./test_pipeline.sh --mode merge         # 3-node devnet + Monero merge mining
./test_pipeline.sh --mode join-native   # Single node joining public testnet
./test_pipeline.sh --mode join-merge    # Single merge-mining node, public testnet

# Build options
#   --no-cache    Rebuild all Docker layers from scratch (default: use cache)
#   --fresh       Aggressive clean: prune images, build cache, buildx (default: off)
#   --with-wallet Build and start wallet Docker container alongside devnet
./test_pipeline.sh --mode merge --no-cache --fresh   # Deterministic full rebuild
./test_pipeline.sh --mode native --with-wallet       # 3-node devnet + wallet container

# Or manually (requires --profile since all services use profiles):
docker compose --profile native up -d    # Start the 3-node stack (native)
docker compose --profile merge up -d     # Start with merge mining

# Check status
docker compose ps

# View logs
docker compose logs -f

# Tear down
docker compose down
```

## Mining

Both node0 and node1 mine via xmrig connecting to their local stratum servers.
Mining address resolution follows a three-tier priority:

1. `WALLET_ADDRESS` environment variable (operator-provided)
2. Persisted file from a prior dwowd run
3. Auto-generated keypair on first dwowd start (if no secret provided)

Block reward follows an exponential-decay emission schedule starting at
~13.84 DRKW at height 1, with a tail emission floor of ~0.80 DRKW.
Total supply cap is 21,000,000 DRKW. The testnet uses auto-adjusting
difficulty with an initial difficulty of 255 and a target block time
of 120 seconds.

## Wallet Setup

The wallet is initialized by `test_pipeline.sh` with the same mining keypair
used by the nodes. No secret extraction from containers is needed — `dww scan`
decrypts coinbase AEAD-encrypted notes client-side using the wallet's local
secret key.

```bash
NETWORK="darkwow-testnet"
DRK="./target/release/dww"

# Verify wallet has keys (test_pipeline.sh must complete first)
$DRK -n $NETWORK wallet address

# Scan the blockchain for coins
$DRK -n $NETWORK scan

# Check balance (should show DRKW from mining rewards)
$DRK -n $NETWORK wallet balance
```

## Wallet Container

A standalone Docker container provides wallet interaction within the localnet.
It builds only `dww` (no WASM contracts, no `dwowd`, no `lilith`) and runs in
two modes: `test` (auto-init, scan, position, assert, exit) for CI, or
`interactive` (`sleep infinity` for `docker exec` access) for dev work.

```bash
# Interactive mode — start alongside the running testnet
docker compose --profile wallet up -d wallet
docker exec dwow-wallet dww wallet address
docker exec dwow-wallet dww scan
docker exec dwow-wallet dww position

# Tear down
docker compose --profile wallet down -v
```

The pipeline's `--with-wallet` flag adds wallet container build, start, and
verify steps to Phases 4, 5, and 6 of `test_pipeline.sh`:

```bash
./test_pipeline.sh --mode native --with-wallet
```

### Automated Wallet Test

[`test-wallet.sh`](../../../contrib/docker/darkwow-testnet/test-wallet.sh) starts the
wallet container in test mode and verifies the full scan-to-position cycle in
five phases: pre-flight checks, container start, wait for completion (up to 120s),
output verification (coin capabilities, descriptors, capabilities section, wallet
address), and cleanup. The container auto-exits 0 on success or 1 on failure.

```bash
./test-wallet.sh
```

For full details see the
[darkwow-testnet README](../../../contrib/docker/darkwow-testnet/README.md#wallet-docker-container).

## Contract Tests

```bash
# Single-contract test (deploy + transfer)
./contract_test.sh

# Multi-contract test (deploy money_v3, DEX, dao_escrow + transfer + fee)
./test-contracts.sh
```

The contract tests exercise the full economic cycle: mining → fund wallet →
deploy WASM contract → transfer tokens → pay fees.

## Dwow-Devnet Variant

A 3-node bridge-networked variant is available at `contrib/docker/dwow-devnet/`
with relaxed parameters for rapid local iteration:

| Feature | `darkwow-testnet` | `dwow-devnet` |
|---------|-------------------|---------------|
| `localnet` | `false` | `false` |
| Magic bytes | `[68, 82, 75, 87]` | auto-derived |
| Threshold | 3 | 1 |
| `pow_target` | 120 | 120 |
| `fixed_difficulty` | auto-adjusting | 1 (instant blocks) |
| `skip_fees` | `false` | `true` |
| `skip_sync` | `false` | `true` |
| Nodes | 3 (seed + 2 miners) | 3 (lilith + 2 miners) |
| Networking | Bridge (port-mapped) | Bridge (default) or Host |

Use `dwow-devnet` for fast local contract testing. Use `darkwow-testnet` when
you need parameters matching the public testnet.

## Base Image

All Docker images in this testnet inherit from `darkwow-base:24.04` — a
pre-baked Ubuntu 24.04 image containing every apt dependency and the Rust
toolchain across all build profiles. The base image is built once (reused
indefinitely), so per-commit Docker builds only pay for git clone + cargo
compile. The test pipeline builds it automatically if missing.

```bash
./contrib/docker/darkwow-testnet/build-base.sh
```

## File Overview

| File | Purpose |
|------|----------|
| `Dockerfile.base` | **Base image** — all apt packages + Rust toolchain. Built once, inherited by all other Dockerfiles |
| `build-base.sh` | Build and optionally push the base image |
| `Dockerfile` | Multi-stage build from base (git clone + cargo: zkas → WASM → dwowd + lilith + xmrig) |
| `Dockerfile.monero` | Monero daemon image using pre-built binary (merge mining). Inherits from base |
| `Dockerfile.p2pool` | p2pool + xmrig image using pre-built binaries. Inherits from base |
| `docker-compose.yml` | Service orchestration with 3 profiles: native, merge, join-merge |
| `entrypoint.sh` | Dynamic TOML config generation for lilith and dwowd roles; spawns xmrig for native mining |
| `entrypoint-p2pool.sh` | Start p2pool + xmrig in merge mining mode (Monero parent + DarkWow aux) |
| `entrypoint-monero.sh` | Start monerod for merge mining (offline or connected mode) |
| `build-and-push.sh` | Build and optionally push image to a registry |
| `join-testnet.sh` | Launch a single node joining the public DarkWow testnet (native or merge) |
| `test_pipeline.sh` | Single entry point: 4 modes (native, merge, join-native, join-merge), 10-12 phases each. Auto-builds base image if missing |
| `test-contracts.sh` | Multi-contract deploy and transaction test |
| `contract_test.sh` | Single-contract deploy + transfer test |
| `Dockerfile.wallet` | Wallet container — builds only `dww` (no WASM, no dwowd, no lilith). Fast build (~5min) |
| `entrypoint-wallet.sh` | Wallet entrypoint — generates `drk.toml`, imports/generates keypair, dispatches test/interactive mode |
| `test-wallet.sh` | Level 3 wallet container integration test — starts container in test mode, verifies position output |

See the [darkwow-testnet README] for the full modes comparison table, Docker
image catalog, compose profile reference, and current pass/fail counts for all
five pipeline modes.

[darkwow-testnet README]: https://github.com/darkrenaissance/darkfi/blob/master/contrib/docker/darkwow-testnet/README.md
