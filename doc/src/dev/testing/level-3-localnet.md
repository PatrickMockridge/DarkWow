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

# Full pipeline — 4 modes (clean → build → verify)
./test_pipeline.sh --mode native        # 6-node local devnet, native mining (--nodes 2 default)
./test_pipeline.sh --mode merge         # 5-node devnet + Monero merge mining
./test_pipeline.sh --mode join-native   # Single node joining public testnet
./test_pipeline.sh --mode join-merge    # Single merge-mining node, public testnet

# Build options
#   --no-cache    Rebuild all Docker layers from scratch (default: use cache)
#   --fresh       Aggressive clean: prune images, build cache, buildx (default: off)
#   --with-wallet Build and start wallet Docker container alongside devnet
./test_pipeline.sh --mode merge --no-cache --fresh   # Deterministic full rebuild
./test_pipeline.sh --mode native --with-wallet       # 3-node devnet + wallet container

# Or manually (requires --profile since all services use profiles):
# NOTE: compose up starts ALL services in the profile. Use test_pipeline.sh
# with --nodes to control how many mining nodes are started.
docker compose --profile native up -d    # Start the full stack (6 containers)
docker compose --profile merge up -d     # Start merge mining (5 containers)

# Check status
docker compose ps

# View logs
docker compose logs -f

# Tear down
docker compose down
```

## Mining

Native mode uses the built-in miner (dwowd's internal `miner_task`). Merge mode
adds p2pool + xmrig as sidecars inside each mining node container.

Mining address resolution follows a three-tier priority:
1. `FORWARD_DESTINATION` — redirects coinbase rewards directly (wallet testing)
2. `WALLET_ADDRESS` + `WALLET_SECRET` — operator-provided mining keypair
3. Auto-generated keypair on first dwowd start (if no secret provided)

Block reward follows an exponential-decay emission schedule starting at
~13.84 DRKW at height 1, with a tail emission floor of ~0.80 DRKW.
Total supply cap is 21,000,000 DRKW. The testnet uses auto-adjusting
difficulty with an initial difficulty of 255 and a target block time
of 120 seconds.

## Wallet Setup

The wallet runs as a Docker container on the bridge network, same as the mining
nodes. It syncs the chain via P2P (GetTip/GetBlocks), scans blocks locally with
AEAD decryption, and discovers coinbase rewards. Zero RPC.

### Secret Provisioning

For the wallet to decrypt coinbase outputs, its secret key MUST match the
`FORWARD_DESTINATION` address. The host wallet's secret is written to
`/tmp/dwow_mining_secret` before starting the pipeline. The pipeline bind-mounts
this file into the wallet container at `/run/secrets/mining_secret:ro`
(test_pipeline.sh line 902). The entrypoint imports it via
`wallet import-secrets` (entrypoint-wallet.sh lines 82-100).

```
Host                                      Docker
────                                      ─────
wallet keygen → secret_hex                FORWARD_DESTINATION=<addr>
  │                                         │
  ├── /tmp/dwow_mining_secret ──mount──▶ /run/secrets/mining_secret
  │                                         │
  │                                    entrypoint-wallet.sh:
  │                                      xxd -r -p → bs58 → import-secrets
  │                                         │
  │                                    wallet.address() must match FORWARD_DESTINATION
  │
  └── FORWARD_DESTINATION=addr ──env──▶ Mining nodes encrypt coinbase to addr
```

This is a conscious deviation from the ideal zero-secret-sharing model. In
production, the wallet operator generates a keypair, publishes the address,
and imports the secret into their wallet. The pipeline's wallet container
models this exactly: the operator (host) generates the keypair and provisions
the secret to the wallet container.

### Key Copy Policy — CRITICAL

The ONLY key that may be copied into a container is the **wallet forwarding key**
— the secret corresponding to `FORWARD_DESTINATION`. This key decrypts coinbase
outputs. It is NEVER a mining key.

**Permitted**: Wallet secret → wallet container (for AEAD decryption of coinbase)
**FORBIDDEN**: Mining secret → wallet container (hot-wallet breach, miner can spend wallet coins)
**FORBIDDEN**: Wallet secret → mining node container (miner never needs to spend from the wallet)

The mining node generates its own independent keypair for block signing. The
wallet generates its own independent keypair for coinbase decryption and spending.
`FORWARD_DESTINATION` bridges them: the miner encrypts coinbase to the wallet's
public key. The miner never sees the wallet's secret. The wallet never sees the
miner's secret. This is the production pattern.

The bind-mount `/tmp/dwow_mining_secret:/run/secrets/mining_secret:ro` is
misleadingly named — it carries the WALLET key, not a mining key. The file
name is historical; the content is a wallet forwarding key only.

See [Wallet Architecture](../../arch/wallet.md) for the full o-cap model.

### Shell Interaction

Use `wallet-shell.sh` — a sourceable library matching the pattern from
`test-wallet-transactions.sh`:

```bash
source contrib/docker/darkwow-testnet/wallet-shell.sh

# Verify wallet address matches FORWARD_DESTINATION
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
It builds only `dwow_wallet` (no WASM contracts, no `dwowd`, no `lilith`) and runs in
two modes: `test` (auto-init, scan, position, assert, exit) for CI, or
`interactive` (`sleep infinity` for `docker exec` access) for dev work.

The container runs on the `dwow-local` bridge network — same as lilith, node0,
and node1. It connects to lilith at `tcp+tls://lilith:31340` (Docker DNS),
discovers peers via hostlist, syncs blocks via GetTip/GetBlocks, and scans
locally. Container name: `dwow-wallet-N`.

```bash
# Start via pipeline (builds image, starts container, provisions secret)
FORWARD_DESTINATION="<wallet_address>" \
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
verify steps to Phases 4, 5, and 6 of `test_pipeline.sh`. The first wallet
container (N=1) receives the mining secret via bind-mount from
`/tmp/dwow_mining_secret`.

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

# Multi-contract test (deploy 25 contracts + transfer + fee)
./test-contracts.sh --mode native --tier 2

# All per-contract wallet tests (17 contracts, individual scripts)
./contract-tests/run-all.sh
```

### Wallet Funding via Coinbase Forwarding

To test contracts, the wallet needs coins from mining. Coinbase forwarding
redirects mining rewards to the wallet. The wallet runs as a Docker container
on the bridge network and MUST have the matching secret key to decrypt the
AEAD-encrypted coinbase notes:

```bash
# 1. Generate wallet keypair on host
./target/release/dwow_wallet -n darkwow-testnet wallet initialize
./target/release/dwow_wallet -n darkwow-testnet wallet keygen
WALLET_ADDR=$(./target/release/dwow_wallet -n darkwow-testnet wallet address | tail -1)
WALLET_SECRET=$(./target/release/dwow_wallet -n darkwow-testnet wallet keygen 2>&1 | grep "Secret (hex)" | awk '{print $NF}')

# 2. Write secret for pipeline to mount into wallet container
echo -n "$WALLET_SECRET" > /tmp/dwow_mining_secret

# 3. Pipeline: mining nodes forward coinbase to wallet address
#    Wallet container imports secret, syncs chain, scans blocks
FORWARD_DESTINATION="$WALLET_ADDR" \
  ./test_pipeline.sh --mode native --with-wallet 1 --fresh

# 4. After pipeline: interact with wallet container
source wallet-shell.sh
wal 1 sync init
wal 1 sync status
wal 1 scan
wal 1 wallet balance   # DRKW > 0
```

The secret provisioning step (echo to `/tmp/dwow_mining_secret`) is critical.
Without it, the wallet container generates its own random keypair which does
NOT match `FORWARD_DESTINATION`, and AEAD decryption silently fails — the
wallet scans blocks but finds zero coins.

Mining nodes encrypt coinbase outputs to the wallet's public key. The wallet
decrypts them using its secret key via ChaCha20Poly1305 + Sapling DH. See
[Coinbase Reward Forwarding](../../arch/mining-tokenomics.md#coinbase-reward-forwarding)
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
| `test_pipeline.sh` | Single entry point: 6 modes (native, merge, bridge, join-native, join-merge, wallet), 10-16 phases. Auto-builds base image if missing |
| `test-contracts.sh` | Multi-contract deploy and transaction test |
| `contract_test.sh` | Single-contract deploy + transfer test |
| `contract-tests/run-all.sh` | Orchestrates all 17 per-contract wallet tests |
| `contract-tests/common.sh` | Shared wallet interaction library (deploy, register, invoke, assert) |
| `Dockerfile.wallet` | Wallet container — builds only `dwow_wallet` (no WASM, no dwowd, no lilith). Fast build (~5min) |
| `entrypoint-wallet.sh` | Wallet entrypoint — generates wallet config, imports/generates keypair, dispatches test/interactive mode |
| `wallet-shell.sh` | Sourceable shell library — `wal()` function for consistent `docker exec` wallet interaction |
| `test-wallet.sh` | Level 3 wallet container integration test — starts container in test mode, verifies position output |

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

The embedded wallet config at `bin/drk/dww_config.toml` sets `localnet = true`
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

The `FORWARD_DESTINATION` mechanism decouples them: the miner encrypts the
coinbase to the wallet's public key. The miner never needs the wallet's secret.
Using the same key for both creates a hot-wallet — the mining key (always online)
can spend the wallet's coins.

The `entrypoint.sh` and `entrypoint-wallet.sh` scripts accept both patterns via
`WALLET_SECRET` and `FORWARD_DESTINATION`. When both are set to different
addresses, the miner signs with its own key and forwards rewards to the wallet.
This is the production pattern.

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
- [ ] `FORWARD_DESTINATION` set to a wallet address that the miner does NOT control
- [ ] TLS certificates configured (not self-signed) or explicitly pinned
- [ ] Magic bytes match the target network
- [ ] `sync status` verified to show network tip after seed connection
- [ ] Transaction broadcast + confirmation tested end-to-end

See the [darkwow-testnet README] for the full modes comparison table, Docker
image catalog, compose profile reference, and current pass/fail counts for all
five pipeline modes.

[darkwow-testnet README]: https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/contrib/docker/darkwow-testnet/README.md
