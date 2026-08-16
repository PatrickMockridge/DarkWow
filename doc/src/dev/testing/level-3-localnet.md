# Level 3: Containerized Localnet

A multi-container Docker testnet that mirrors public testnet conditions. The
default `--nodes 2` starts 3 containers (seed + 2 mining nodes). `--nodes 5`
starts 6 containers (seed + 5 mining nodes). Each mining node is self-contained:
built-in miner for native mode, p2pool+xmrig sidecars for merge mode.

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

# Full pipeline — 6 modes (clean → build → verify)
./test_pipeline.sh --mode native        # Local devnet, native mining (--nodes 1|2|5, default 2)
./test_pipeline.sh --mode merge         # Devnet + Monero merge mining (monerod + p2pool + xmrig sidecars)
./test_pipeline.sh --mode bridge        # Devnet + bridge relay node (full bridge lifecycle: deploy→execute)
./test_pipeline.sh --mode join-native   # Single node joining public testnet, native mining
./test_pipeline.sh --mode join-merge    # Single merge-mining node, public testnet
./test_pipeline.sh --mode wallet        # Build wallet image + generate keypair, then exit

# Build options
#   --no-cache       Rebuild all Docker layers from scratch (default: use cache)
#   --fresh          Aggressive clean: prune images, build cache, buildx (default: off)
#   --skip-build     Skip Docker build — use cached images
#   --build-local    Build from local working tree (COPY .) instead of cloning from origin
#   --resume-from N  Resume from phase N (skip phases 1 through N-1, validates preconditions)
#   --stop-after N   Run phases 1 through N, then print report and exit
#   --phase N        Run a single phase with precondition validation (requires running devnet)
#   --with-wallet N  Build and start N wallet containers alongside devnet (0-5)
#   --contract-tier N Run contract E2E tests after pipeline (1-4)
#   --nodes N        Native mining nodes: 1, 2, or 5 (native mode only)

# Full rebuild from origin (deterministic CI)
./test_pipeline.sh --mode merge --no-cache --fresh

# Local dev iteration (build from working tree, skip to just the phases you need)
./test_pipeline.sh --mode native --build-local --stop-after 9    # Build+test through blocks, then stop
./test_pipeline.sh --mode native --phase 10                      # Run wallet verify against running devnet
./test_pipeline.sh --mode native --with-wallet 2 --build-local   # Full wallet test from local code

# Wallet + contract testing
./test_pipeline.sh --mode native --with-wallet 2 --contract-tier 2  # Full cycle: wallet + deploy + invoke

# Or manually (requires --profile since all services use profiles):
# NOTE: compose up starts ALL services in the profile. Use test_pipeline.sh
# with --nodes to control how many mining nodes are started.
docker compose --profile native up -d    # Start the full stack (6 containers)
docker compose --profile merge up -d     # Start merge mining (5 containers)

### Composable Workflow

The pipeline supports running subsets of phases without re-running everything:

```bash
# Start a devnet and stop after block production (skip wallet/contract tests)
./test_pipeline.sh --mode native --stop-after 9

# Re-run wallet verification against the running devnet
./test_pipeline.sh --mode native --phase 10

# Re-run wallet transfer test
./test_pipeline.sh --mode native --phase 11

# Run standalone wallet tests (no pipeline overhead)
./test-wallet-against-devnet.sh

# Run contract tests against the running devnet
./test-contracts.sh --mode native --tier 2

# Build from local working tree (no push required)
./test_pipeline.sh --mode native --build-local --stop-after 9

# Resume from phase 12 with precondition validation
./test_pipeline.sh --mode bridge --resume-from 12
# Fails immediately if BRIDGE_HELPER is missing or node0 RPC is unreachable
```

**Precondition validation:** `--resume-from` and `--phase` validate that
the required state exists before executing. If you `--resume-from 12`
without running phases 1-11 first, you get: "Precondition:
bridge_test_helper not found. Run phase 3 (prereqs) first."

# Check status
docker compose ps

# View logs
docker compose logs -f

# Tear down
docker compose down
```

## Production Fidelity

The pipeline is a **production test infrastructure**, not a developer
convenience tool. Every component mirrors mainnet conditions:

- **Real PoW**: RandomX at production difficulty. No simulated mining.
  120-second target block time. Blocks are actually mined.
- **Real P2P**: Full TLS certificate validation between nodes. Docker
  bridge networking with hostname-based service discovery. No mocked
  network layers.
- **Real Docker**: Full build from source — git clone from origin,
  cargo build --release, zkas proof regeneration for 30+ contracts.
  No pre-built binaries outside the base image.
- **Real merge mining**: xmrig sidecar → p2pool stratum → monerod
  daemon. Cryptographic receipt verification polls actual xmrig output.
- **Real bridge lifecycle**: Deploy → init → register → deposit →
  withdraw → accept → execute → verify. All 8 phases with ZK proofs.

A full native-mode run is 20-40 minutes from cold cache. Merge mining
adds 10-30 minutes for xmrig to find Monero shares. This is not a bug —
it's the actual pace of the production network. The pipeline's job is to
find failures, not to be fast.

**Ecosystem responsibility:** Quick iteration belongs in the ecosystem,
not the core repository. Use Python simulations for contract state
machines. Use `cargo test` for unit tests. Use `cargo test --release`
for ZK proof tests. The Level 3 pipeline is the final gate before
Level 4 (public testnet) and ultimately mainnet deployment.

## Mining

Native mode uses the built-in miner (dwowd's internal `miner_task`). Merge mode
adds p2pool + xmrig as sidecars inside each mining node container.

Mining keys are **declared, never synthesized at boot**. Each node reads its identity from
the `[NODE_NAME]` section of `keys.toml` (passed via `--keys /run/config/keys.toml`); a
missing section is a hard error — there is no auto-generation. The node mines coinbase
rewards to its own declared key. The deprecated `FORWARD_DESTINATION` env var is no longer
consumed by the node.

Block reward follows an exponential-decay emission schedule starting at
~13.84 DRKW at height 1, with a tail emission floor of ~0.80 DRKW.
Total supply cap is 21,000,000 DRKW. The testnet uses auto-adjusting
difficulty with an initial difficulty of 255 and a target block time
of 120 seconds.

## Wallet Setup

The wallet runs as a Docker container on the bridge network, same as the mining
nodes. It syncs the chain via P2P (GetTip/GetBlocks), scans blocks locally with
AEAD decryption, and discovers coinbase rewards. Zero RPC.

### Key Declaration (keys.toml)

The wallet derives its identity from `keys.toml` on boot — no addresses table, no import
step. `entrypoint-wallet.sh` exports `WALLET_NAME` (section name, default `wallet-1`) and
`KEYS_FILE` (`/run/config/keys.toml`), then every wallet invocation resolves its key via
that declaration (mirroring dwowd's `--keys` + `NODE_NAME`). The entrypoint hard-fails if
`keys.toml` is missing.

For the wallet to decrypt coinbase outputs, its declared secret MUST match the key the
mining node encrypts to. In the pipeline, wallet-1 and node0 declare the **same** secret in
the shared `keys.toml` — key sharing is by **declaration**, not export/import. The wallet is
funded because it holds node0's declared secret and decrypts the coinbase during scan.

> **Deprecated:** `FORWARD_DESTINATION` (redirect coinbase to an external wallet address) is
> no longer consumed by the node or wallet. The older three-tier mining-address priority and
> the hex→bs58→`wallet import-secrets` flow are gone; keys are declared in `keys.toml`.

### Key Copy Policy — CRITICAL

In local testing, wallet-1 and node0 share one `keys.toml` secret for convenience. This is a
**hot-wallet**: the mining key (always online) can decrypt and spend the wallet's coins. In
production, the mining keypair and wallet keypair MUST be separate, with coinbase rewards
paid to a wallet address the miner does not control. See
[Local Docker → Public Testnet → Mainnet Transition](#local-docker--public-testnet--mainnet-transition).

See [Wallet Architecture](../../arch/wallet.md) for the full o-cap model.

### Shell Interaction

Use `wallet-shell.sh` — a sourceable library matching the pattern from
`test-wallet-transactions.sh`:

```bash
source contrib/docker/darkwow-testnet/wallet-shell.sh

# Verify wallet address (derived from the [wallet-1] keys.toml declaration)
wal 1 wallet address

# Wallet P2P sync
wal 1 sync init
wal 1 sync status

# Scan local chain, decrypt coinbase
wal 1 scan

# Check balance
wal 1 wallet balance
```

`wal()` wraps: `docker exec "dwow-wallet-$i" /app/dwow_wallet "$@"`

## Wallet Container

A standalone Docker container provides wallet interaction within the localnet.
It builds only the wallet binary (no WASM contracts, no `dwowd`, no `lilith`) and runs
`wallet wallet initialize` followed by `wallet daemon` (continuous P2P sync + scan). The
wallet's identity is derived from the `[wallet-N]` section of the mounted `keys.toml`.

The container runs on the `dwow-local` bridge network — same as lilith, node0,
and node1. It connects to lilith at `tcp+tls://lilith:31340` (Docker DNS),
discovers peers via hostlist, syncs blocks via GetTip/GetBlocks, and scans
locally. Container name: `dwow-wallet-N`.

```bash
# Start via pipeline (builds image, starts container; key declared in keys.toml)
./test_pipeline.sh --mode native --with-wallet 1 --fresh

# Interact via wallet-shell.sh
source wallet-shell.sh
wal 1 sync init
wal 1 sync status
wal 1 scan
wal 1 wallet balance

# Or directly via docker exec
docker exec dwow-wallet-1 /app/dwow_wallet scan

# Tear down
docker compose --profile wallet down -v
```

The pipeline's `--with-wallet N` flag adds wallet container build, start, and
verify steps to Phases 2 (build), 4 (wallet keygen), 5 (start), 6 (verify),
10 (wallet verify — sync/scan/balance), and 11 (wallet transfer). Each wallet
container resolves its identity from the `[wallet-N]` section of the shared
`keys.toml` (mounted at `/run/config/keys.toml`).

### Automated Wallet Test

[`test-wallet.sh`](../../../contrib/docker/darkwow-testnet/test-wallet.sh) starts the
wallet container and verifies its logs in five phases: pre-flight checks, container start,
wait (up to 120s), log verification (coin capabilities, descriptors, capabilities section,
wallet address), and cleanup. Note that the current `entrypoint-wallet.sh` always runs
`init` + `daemon` (there is no `WALLET_MODE=test` auto-exit path), so the script inspects
captured container logs rather than relying on self-termination.

```bash
./test-wallet.sh
```

For full details see the
[darkwow-testnet README](../../../contrib/docker/darkwow-testnet/README.md#wallet-docker-container).

## Contract Tests

```bash
# Single-contract test (deploy + transfer)
./contract_test.sh

# Multi-contract test (deploy 25 contracts + transfer + fee)
./test-contracts.sh --mode native --tier 2

# All per-contract wallet tests (17 contracts, individual scripts)
./contract-tests/run-all.sh
```

### Wallet Funding via Shared Key Declaration

To test contracts, the wallet needs coins from mining. The wallet and the mining node
(node0) declare the **same** secret in the shared `keys.toml`, so the wallet can decrypt the
node's AEAD-encrypted coinbase notes during scan:

```bash
# The shared keys.toml (mounted at /run/config/keys.toml) declares both
# [node0] (mining) and [wallet-1] (wallet) with the SAME secret.

# 1. Pipeline: start devnet with a wallet container
./test_pipeline.sh --mode native --with-wallet 1 --fresh

# 2. After pipeline: interact with the wallet container
source wallet-shell.sh
wal 1 sync init
wal 1 sync status
wal 1 scan
wal 1 wallet balance   # DRKW > 0
```

The critical requirement is that `wallet-1`'s declared secret matches `node0`'s declared
secret. If they differ, AEAD decryption silently fails — the wallet scans blocks but finds
zero coins.

Mining nodes encrypt coinbase outputs to their declared key's public key. The wallet decrypts
them using its declared secret via ChaCha20Poly1305 + Sapling DH. See
[Coinbase Reward Forwarding](../../arch/consensus-coinbase.md#coinbase-reward-forwarding)
and [Wallet Architecture](../../arch/wallet.md).

The contract tests exercise the full economic cycle: mining → fund wallet →
deploy WASM contract → transfer tokens → pay fees.

## Dwow-Devnet Variant

A 3-node bridge-networked variant is available at `contrib/docker/darkwow-devnet/`
with relaxed parameters for rapid local iteration:

| Feature | `darkwow-testnet` | `darkwow-devnet` |
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

Use `darkwow-devnet` for fast local contract testing. Use `darkwow-testnet` when
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
| `docker-compose.yml` | Service orchestration with 5 profiles: native, merge, bridge, join-merge, wallet |
| `entrypoint.sh` | Dynamic TOML config generation for lilith and dwowd roles; spawns xmrig for native mining |
| `entrypoint-p2pool.sh` | Start p2pool + xmrig in merge mining mode (Monero parent + DarkWow aux) |
| `entrypoint-monero.sh` | Start monerod for merge mining (offline or connected mode) |
| `build-and-push.sh` | Build and optionally push image to a registry |
| `join-testnet.sh` | Launch a single node joining the public DarkWow testnet (native or merge) |
| `test_pipeline.sh` | Thin orchestrator (~230 lines) — sources 18 `lib/*.sh` modules, dispatches sequential phases across 6 modes, 4-21 phases |
| `lib/output.sh` | Display functions: `info`, `warn`, `error`, `pass`, `fail`, `check` + `PASS`/`FAIL` counters |
| `lib/traps.sh` | Error handling: `set -eE`, ERR/signal/EXIT traps, `cleanup_on_exit()` |
| `lib/config.sh` | All configuration: `usage()`, flag parsing, validation, constants, `DWW()` wallet wrapper, log capture |
| `lib/helpers.sh` | Shared utilities: `clean_data_dir`, `is_join_mode`, `is_bridge_mode`, `check_image`, `check_network`, `jsonrpc`, `_verify_height_via_rpc`, `report` |
| `lib/phase_01_clean.sh` through `lib/phase_99_contract_tests.sh` | 14 phase modules — one per dispatch phase pair (local + join variants) |
| `pipeline_spec.py` | Python architecture specification — 50 functions across 18 modules, source of truth for modularization |
| `test-contracts.sh` | Multi-contract deploy and transaction test |
| `contract_test.sh` | Single-contract deploy + transfer test |
| `contract-tests/run-all.sh` | Orchestrates all 17 per-contract wallet tests |
| `contract-tests/common.sh` | Shared wallet interaction library (deploy, register, invoke, assert) |
| `Dockerfile.wallet` | Wallet container — builds only `dwow_wallet` (no WASM, no dwowd, no lilith). Fast build (~5min) |
| `entrypoint-wallet.sh` | Wallet entrypoint — generates wallet config, imports/generates keypair, dispatches test/interactive mode |
| `wallet-shell.sh` | Sourceable shell library — `wal()` function for consistent `docker exec` wallet interaction |
| `test-wallet.sh` | Level 3 wallet container integration test — starts container, verifies coin/descriptor/capability/address log output |

## Local Docker → Public Testnet → Mainnet Transition

When publishing containers to Docker Hub or moving from local devnet to public
networks, four configuration differences must be understood. The wallet binary
and P2P protocol are identical across all environments — only the config changes.

### Config Differences by Environment

| Setting | Local Docker | Public Testnet | Mainnet |
|---------|-------------|----------------|---------|
| `localnet` | `true` | `false` | `false` |
| Seeds | `tcp+tls://lilith:31340` (Docker DNS) | `lilith0.dark.fi:18340`, `lilith1.dark.fi:18340` | TBD |
| TLS verification | Disabled (self-signed Docker certs) | Full (public CA or pinned) | Full (public CA or pinned) |
| Hostlist addresses | Docker hostnames (`node0`, `node1`) | Public IPs / DNS | Public IPs / DNS |
| DNS resolution | Docker embedded DNS | System DNS | System DNS |
| Magic bytes | `[68, 82, 75, 87]` | `[68, 82, 75, 87]` | TBD |
| `active_profiles` | `["tcp+tls"]` | `["tcp+tls"]` | `["tcp+tls"]` |

### Pattern A: `localnet = true`

The embedded wallet config at `bin/dww/dww_config.toml` sets `localnet = true`
for `darkwow-testnet` (line 125). This disables TLS certificate name verification
and allows private IP addresses in the hostlist filter. It is REQUIRED for Docker
because container hostnames (`lilith`, `node0`) don't match self-signed cert CNs.

**For public testnet and mainnet, `localnet` MUST be `false`.** Running with
`localnet = true` on a public network disables TLS certificate validation — a
MITM vector. The wallet would accept any self-signed certificate from any peer.

The wallet's `testnet` section (line 42) does NOT set `localnet = true` — it is
safe for public use. The `mainnet` section (line 139) has `active_profiles = []`
and must be explicitly configured before mainnet launch.

### Pattern B: Secret Sharing (Miner + Wallet)

In local testing, the same keypair is used for mining (block signing) and the
wallet (coinbase decryption). This is a testing convenience. **In production,
these MUST be separate keypairs:**

- **Mining keypair**: Generated by the mining node on first start or configured
  by the operator. Signs coinbase proofs. Must be online for mining.
- **Wallet keypair**: Generated by the wallet holder. Decrypts coinbase outputs
  and signs spend transactions. Can be offline (cold storage).

Production decouples the two keypairs by paying coinbase rewards to a wallet address the
miner does not control (the miner encrypts to the wallet's public key; it never holds the
wallet's secret). Using the same key for both creates a hot-wallet — the mining key (always
online) can spend the wallet's coins. The current pipeline shares one `keys.toml` secret
between `node0` and `wallet-1` (a testing convenience), which does NOT model this separation.

### Pattern C: Transaction Broadcast Confirmation

The test flow verifies sync → scan → balance. It does NOT test the full
spend cycle: build tx → P2P broadcast → mine in block → confirm. This is
a known gap. For production readiness, add a spend confirmation test:

```bash
# After wallet-1 has coins:
wal 2 wallet address  # get wallet-2 address
wal 1 transfer 1.0 DRKW <wallet-2-address>
# Wait for next block (120s)
wal 2 scan
wal 2 wallet balance  # should show 1.0 DRKW
```

The `broadcast_tx` P2P path (`lib.rs` broadcast_tx → `p2p.broadcast(&TxMessage)`)
is wired but untested in CI.

### Pattern D: Seed Connection Failure Detection

`p2p.seed().await` in `init_p2p()` returns `()` — seed connection errors are
logged but not propagated. A wallet with misconfigured seeds reports
"P2P connected: yes" but has zero peers. For production:

- Verify `sync status` shows `network tip > 0` after `sync init`
- Consider changing `seed()` to return `Result<()>` so `init_p2p()` can report
  "connected to 0 of N seeds" instead of silent success
- The sync task already detects zero peers and logs "No peers available" —
  surface this to `sync status` output

### Transition Checklist

Before publishing containers to Docker Hub or connecting to public networks:

- [ ] `localnet` set to `false` for public testnet and mainnet configs
- [ ] Seed addresses updated to public DNS names
- [ ] Mining keypair and wallet keypair are separate (different secrets)
- [ ] Coinbase rewards paid to a wallet address the miner does NOT control
- [ ] TLS certificates configured (not self-signed) or explicitly pinned
- [ ] Magic bytes match the target network
- [ ] `sync status` verified to show network tip after seed connection
- [ ] Transaction broadcast + confirmation tested end-to-end

See the [darkwow-testnet README] for the full modes comparison table, Docker
image catalog, compose profile reference, and current pass/fail counts for all
five pipeline modes.

[darkwow-testnet README]: https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/contrib/docker/darkwow-testnet/README.md
