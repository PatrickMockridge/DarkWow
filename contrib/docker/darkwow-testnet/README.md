# DarkWow Testnet — Containerized Devnet

A 3-node Docker devnet (lilith seed + 2 mining fullnodes) suitable for local
development, multi-machine LAN deployment, and public internet devnets. Magic
bytes `[68, 82, 75, 87]` encode "DRKW" in ASCII, uniquely identifying DarkWow
nodes on the P2P network.

## Quick Start

```bash
# Build and start all 3 containers
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml up -d

# Check status
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml ps

# View logs
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml logs -f

# Tear down
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml down
```

## Joining the Public Testnet

To join the existing public DarkWow testnet as an external participant, use the
`join-testnet.sh` script. This launches a single mining node (not the full 3-node
devnet) that connects to the public lilith seeds.

```bash
# Native mining (solo) — xmrig mines via dwowd's built-in stratum
./contrib/docker/darkwow-testnet/join-testnet.sh --mode native

# Merge mining — xmrig mines via p2pool, submits to Monero testnet + DarkWow
./contrib/docker/darkwow-testnet/join-testnet.sh --mode merge
```

The script auto-detects your public IP for P2P reachability and sets sensible
defaults for the public testnet (magic bytes `[68, 82, 75, 87]`, threshold 3,
block time 120s, public seeds). Override any default via CLI flags or environment
variables.

The node remembers peers it has connected to via a persistent hostlist file
stored in the data directory (`./data/dwowd/`). On restart, it reconnects to
known peers without relying solely on the seed nodes.

Prerequisites:
- Docker image built or pulled (`darkwow-testnet:latest`)
- A DarkWow wallet address for mining rewards
- For merge mode: a Monero testnet wallet address

See `./join-testnet.sh --help` for all options.

## Architecture

Three containers on a bridge network (`dwow-local`):

| Container | Role | P2P Port | RPC Port | Stratum Port |
|-----------|------|----------|----------|--------------|
| `dwow-lilith` | P2P seed (lilith) | 31340 | — | — |
| `dwow-node0` | Mining fullnode (dwowd + xmrig) | 31342 | 31345 | 31347 |
| `dwow-node1` | Mining fullnode (dwowd + xmrig) | 31343 | 31346 | 31348 |

Each mining node connects to lilith as its P2P seed, plus the other mining node
as a direct peer. xmrig mines via local stratum (RandomX `rx/0`), and coinbase
rewards are paid to an auto-generated mining address.

## Network Parameters

| Parameter | Value |
|-----------|-------|
| Block time | 120 seconds |
| Initial difficulty | 255 (auto-adjusting) |
| PoW algorithm | RandomX (rx/0) |
| Consensus threshold | 3 |
| Magic bytes | `[68, 82, 75, 87]` ("DRKW") |
| `localnet` | `false` |
| `skip_fees` | `false` |
| `skip_sync` | `false` |

## Multi-Machine LAN Deployment

Each machine runs one container with host networking. The seed machine must be
reachable from all other machines.

### Machine 1: Seed (lilith)

```bash
docker run --rm --network=host \
  -e ROLE=lilith \
  -e NETWORK=darkwow-testnet \
  -e P2P_PORT=31340 \
  -e MAGIC_BYTES=68,82,75,87 \
  -v /data/lilith:/root/.local/share/dwow/lilith \
  darkwow-testnet:latest
```

### Machine 2: Mining Node

```bash
docker run --rm --network=host \
  -e ROLE=dwowd \
  -e NETWORK=darkwow-testnet \
  -e P2P_PORT=31342 \
  -e RPC_PORT=31345 \
  -e STRATUM_PORT=31347 \
  -e SEED_ADDR=<seed-lan-ip>:31340 \
  -e EXTERNAL_ADDR=<this-machine-ip>:31342 \
  -e MAGIC_BYTES=68,82,75,87 \
  -e MINING_THREADS=4 \
  -v /data/node0:/root/.local/share/dwow/dwowd \
  darkwow-testnet:latest
```

Replace `<seed-lan-ip>` with the seed machine's LAN IP (e.g., `192.168.1.10`).
Replace `<this-machine-ip>` with this machine's LAN IP.
Additional mining nodes follow the same pattern with unique `P2P_PORT`,
`RPC_PORT`, and `STRATUM_PORT`.

## Opening to the Internet

To allow external participants (outside your LAN) to join:

1. **On the seed machine**: set up port forwarding on your router for the P2P
   port (default 31340).
2. **Set `EXTERNAL_ADDR`** on the seed:
   ```bash
   docker run --network=host \
     -e ROLE=lilith \
     -e EXTERNAL_ADDR=<your-public-ip>:31340 \
     darkwow-testnet:latest
   ```
3. **External participants** join with `join-testnet.sh` (recommended) or manually:
   ```bash
   # Recommended: use the join script
   ./contrib/docker/darkwow-testnet/join-testnet.sh --mode native

   # Or manually: pass SEED_ADDR (comma-separated for multiple seeds)
   docker run --network=host \
     -e SEED_ADDR=<seed-1>:31340,<seed-2>:31340 \
     darkwow-testnet:latest
   ```

The node remembers peers across restarts via a persistent hostlist file in its
data directory. Mount a host volume (`-v /data/dwowd:/root/.local/share/dwow/dwowd`)
to persist the hostlist and blockchain data.

## Environment Variable Reference

### All roles

| Variable | Default | Description |
|---|---|---|
| `ROLE` | `dwowd` | `lilith` (P2P seed) or `dwowd` (fullnode) |
| `NETWORK` | `darkwow-testnet` | Network name (determines P2P isolation) |
| `P2P_PORT` | `31342` | P2P inbound listen port |
| `MAGIC_BYTES` | auto | 4 comma-separated bytes (auto-derived from NETWORK if unset) |
| `LOCALNET` | `false` | P2P localnet flag |

### lilith-specific

| Variable | Default | Description |
|---|---|---|
| `LILITH_RPC_PORT` | `18927` | lilith management RPC port |
| `LILITH_DATADIR` | `~/.local/share/dwow/lilith/<network>` | Data directory |

### dwowd-specific

| Variable | Default | Description |
|---|---|---|
| `RPC_PORT` | `31345` | JSON-RPC port |
| `STRATUM_PORT` | `31347` | Stratum mining port |
| `MANAGEMENT_PORT` | `31346` | Management RPC port |
| `SEED_ADDR` | (empty) | Comma-separated seed `host:port` for P2P bootstrap |
| `PEER_ADDR` | (empty) | Comma-separated additional peer `host:port` |
| `EXTERNAL_ADDR` | (empty) | Public `host:port` for internet-facing nodes |
| `IS_SEED` | `false` | Run as seed (no upstream seeds configured) |
| `FIXED_DIFFICULTY` | (empty) | Fixed PoW difficulty (unset for auto-adjusting) |
| `TARGET_BLOCK_TIME` | `120` | Block time target in seconds |
| `MINING_ENABLED` | `true` | Auto-start xmrig mining |
| `MINING_THREADS` | `1` | xmrig thread count |
| `RANDOMX_MAX_THREADS` | `0` | Maximum RandomX VM threads (0 = unlimited) |
| `THRESHOLD` | `3` | Confirmation threshold in blocks |
| `SKIP_SYNC` | `false` | Skip blockchain sync on startup |
| `SKIP_FEES` | `false` | Disable fee verification |
| `WALLET_ADDRESS` | auto | Mining payout address (auto-generated if unset) |
| `WALLET_SECRET_FILE` | (empty) | Path to file containing hex-encoded secret key (preferred) |
| `WALLET_SECRET` | auto | Hex-encoded secret key (deprecated — use WALLET_SECRET_FILE) |
| `MERGE_MINING` | `false` | Enable merge mining via Monero p2pool |
| `MM_RPC_PORT` | `31348` | Merge mining JSON-RPC port (p2pool protocol) |
| `DATADIR` | `~/.local/share/dwow/dwowd/<network>` | Blockchain data directory |

## Building from Source

The build takes 30-60 minutes on a typical machine (8GB RAM, 4 cores). Ensure
sufficient disk space (~15GB for build artifacts).

```bash
# From the repo root:
docker build -t darkwow-testnet . -f contrib/docker/darkwow-testnet/Dockerfile
```

Or use the build script:

```bash
./contrib/docker/darkwow-testnet/build-and-push.sh
```

To build and push to a registry:

```bash
REGISTRY=docker.io/myuser/ IMAGE_NAME=darkwow-testnet \
  ./contrib/docker/darkwow-testnet/build-and-push.sh
```

## Wallet Setup

### Pre-Configured Wallet (Recommended)

Generate a keypair on the host and pass it to the container via a file mount.
The miner sends coinbase rewards directly to a wallet you already control —
no secret extraction needed.

```bash
NETWORK="darkwow-testnet"
DRK="./target/release/dww"

# Generate a keypair
$DRK -n $NETWORK wallet keygen
# Output: address (bs58) and secret (hex)

# Write the secret to a secure temp file
echo -n "<hex-secret>" > /tmp/dwow_mining_secret
chmod 600 /tmp/dwow_mining_secret

# Start the testnet — mount the secret file, pass path via env var
docker run --rm --network=host \
  -e ROLE=dwowd \
  -e NETWORK=darkwow-testnet \
  -e WALLET_ADDRESS="<bs58-address>" \
  -e WALLET_SECRET_FILE=/run/secrets/mining_secret \
  -e SEED_ADDR=<seed-host>:31340 \
  -e MAGIC_BYTES=68,82,75,87 \
  -e MINING_THREADS=4 \
  -v /data/node0:/root/.local/share/dwow/dwowd \
  -v /tmp/dwow_mining_secret:/run/secrets/mining_secret:ro \
  darkwow-testnet:latest

# Clean up the temp file
rm -f /tmp/dwow_mining_secret

# The wallet already has the key — scan for coins
$DRK -n $NETWORK scan
$DRK -n $NETWORK wallet balance
```

### Auto-Generated Keypair

If no `WALLET_SECRET` or `WALLET_SECRET_FILE` is provided, dwowd auto-generates
a random keypair on first startup. The secret exists only inside the container's
datadir. Mining rewards are unspendable until the secret is imported into a wallet.

For production: always pre-generate a keypair and provision it securely (SSH,
config management, or mounted secrets file). The `docker exec cat mining_secret`
pattern is **not recommended** — it exposes the secret in the shell history and
treats the container filesystem as a secrets store.

## Merge Mining (Optional)

The testnet supports Monero merge mining via p2pool as an opt-in feature.
Set `MERGE_MINING=true` to use it; leave unset (default) for native mining.

```
MERGE_MINING=false (default)
  xmrig → dwowd stratum (native RandomX mining)

MERGE_MINING=true
  xmrig → p2pool stratum → monerod (parent chain)
                         → dwowd mm_rpc (aux chain)
```

### Quick Start

```bash
# Start with merge mining (adds monerod + p2pool + xmrig-merge containers)
MERGE_MINING=true docker compose --profile merge up -d

# Check merge mining status
docker logs dwow-p2pool
docker logs dwow-monerod

# Check that blocks are being produced (dwowd uses raw TCP JSON-RPC)
docker exec dwow-node0 bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.last_confirmed_block\",\"params\":[],\"id\":1}" >&3; timeout 3 cat <&3'

# Tear down (stops merge mining containers too)
docker compose --profile merge down
```

By default, `monerod` runs in **offline mode** with fixed difficulty — no need
to sync the real Monero testnet. Set `MONERO_OFFLINE=false` to connect to the
public Monero testnet.

Merge mining uses **two independent wallets on two different curves**:

| Wallet | Curve | Address format | Env var |
|--------|-------|---------------|---------|
| DarkWow | Pallas (pasta) | bs58 | `WALLET_ADDRESS` |
| Monero | Ed25519/Curve25519 | base58 | `MONERO_WALLET_ADDRESS` |

Keys cannot be shared between them — they are different cryptographic curves
with no algebraic conversion. Both wallets earn rewards: Monero coinbase from
the parent chain, DarkWow coinbase from the aux chain.

### Merge Mining Environment Variables

| Variable | Default | Description |
|---|---|---|
| `MERGE_MINING` | `false` | Enable merge mining (`true`/`false`) |
| `MM_RPC_PORT` | `31348` | dwowd merge mining RPC port |
| `MONERO_OFFLINE` | `true` | Run monerod in offline mode (local devnet) |
| `MONERO_NETWORK` | `testnet` | Monero network: `testnet` or `mainnet` |
| `MONERO_FIXED_DIFFICULTY` | `20000` | Fixed difficulty when offline |
| `MONERO_ADD_PEERS` | (empty) | Comma-separated Monero bootstrap `host:port` |
| `MONERO_WALLET_ADDRESS` | (empty) | Monero wallet for p2pool mining rewards |
| `WALLET_ADDRESS` | (empty) | DarkWow wallet for aux chain mining rewards |
| `XMERGE_THREADS` | `1` | xmrig-merge thread count |

## Merge Mining Adaptor (p2pool Bridge)

The `dwow-p2pool-adaptor` bridges p2pool to dwowd's stratum protocol, enabling
merge mining with Monero and the anchoring/finality gadget. DarkWow is a
minority-mined RandomX L1 — in the beginning, it borrows security from Monero's
hashpower through this pathway.

```
xmrig → p2pool → adaptor → dwowd stratum → lilith P2P
```

The adaptor translates dwowd's native stratum interface into monerod-compatible
JSON-RPC (`get_block_template`, `submit_block`, `get_info`). p2pool connects to
the adaptor thinking it's monerod, and the adaptor translates every request into
DarkWow's stratum protocol. This means unmodified p2pool — with its full PPLNS
reward distribution, sidechain, and stratum server — interoperates with DarkWow
without any DarkWow-specific code in p2pool itself.

**Scope boundary:** The adaptor is merge mining / finality gadget infrastructure
— it is not a general-purpose DarkWow mining pool. DarkWow-native pooled mining
(DRKW reward distribution without Monero merge mining) is an ecosystem concern.
This repo provides the node software and the adaptor; pool protocols and reward
distribution schemes use the same stratum interface but are not bundled here.

### Quick Start

```bash
# Full pipeline (build + start + verify):
./test_pipeline.sh --mode native-p2pool

# Or directly with docker compose:
docker compose --profile native-p2pool up -d
```

This starts three containers: `dwow-adaptor` (protocol bridge), `dwow-p2pool-darkwow`
(p2pool), and `dwow-xmrig-p2pool` (xmrig miner).

### Adaptor Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DWOWD_RPC` | `node0:31345` | dwowd JSON-RPC for chain queries |
| `DWOWD_STRATUM` | `node0:31347` | dwowd stratum for block templates |
| `ADAPTOR_LISTEN` | `0.0.0.0:28081` | Where the adaptor listens for p2pool |
| `MONERO_HOST` | `adaptor` | p2pool's monerod endpoint (the adaptor) |
| `MONERO_RPC_PORT` | `28081` | Adaptor's listening port |
| `STRATUM_PORT` | `3333` | p2pool stratum port for xmrig |
| `WALLET_ADDRESS` | *(optional)* | DarkWow wallet for mining rewards |
| `CONNECT_RETRIES` | `30` | Max retries for adaptor→dwowd connection |

See [Merge Mining Adaptor](../../doc/src/testnet/native-p2pool.md) for
bare-metal setup, adaptor RPC reference, block header translation, and
known limitations.

### 2026-05-15: Segfault Fix

Two root causes were identified and fixed for the dwowd exit-139 crash in
native-p2pool mode:

1. **JIT in P2P RandomX VM** (`src/linear/src/blockchain.rs`):
   `dwow_linear::LinearBlockchain::create_vm` used `RandomXFlags::get_recommended_flags()`
   which includes JIT. When the P2P layer received a block broadcast and called
   `block.hash(&vm)`, the JIT-compiled code executed illegal instructions on
   Docker hosts with restricted CPU features. Fixed by masking out
   `RandomXFlags::JIT`, matching the stratum path which already did this.

2. **Adaptor blob layout** (`bin/dwow-p2pool-adaptor/src/translate.rs`):
   The adaptor's `NONCE_OFFSET` was 77, but `BlockHeader::NONCE_OFFSET` is 40.
   p2pool was modifying merkle_root bytes instead of the nonce. Fixed by aligning
   the adaptor's serialization layout with `BlockHeader::to_mining_blob()`.

Defense-in-depth: PoW validation was added to `stratum_submit_linear` so invalid
nonces are rejected before chain insertion.

## Test Pipeline

`test_pipeline.sh` is the single entry point for all builds and tests. Every mode
builds Docker images, starts the stack, and runs 10-12 sequential verification
phases. Every check reports PASS or FAIL — there are no skipped or silent checks.

### Modes

| Mode | Type | Profile | Services | Phases | PASS |
|------|------|---------|----------|--------|------|
| `native` | 3-node local devnet | `native` | 3 (lilith, node0, node1) | 10 | 18 |
| `merge` | 3-node local devnet + Monero merge mining | `merge` | 6 (native + monerod, p2pool, xmrig-merge) | 10 | 23 |
| `native-p2pool` | 3-node local devnet + adaptor pathway | `native-p2pool` | 6 (native + adaptor, p2pool-darkwow, xmrig-p2pool) | 10 | TBD |
| `join-native` | Single node joining public testnet | — (docker run, host net) | 1 | 12 | 34 |
| `join-merge` | Single merge-mining node, public testnet | `join-merge` | 4 | 12 | 42 |

```bash
# Local 3-node devnet (10 phases each)
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode merge
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native-p2pool

# Join public testnet as a single node (12 phases each)
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode join-native
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode join-merge
```

**Sequential determinism**: Every phase runs to completion before the next begins.
No background tasks, no parallel operations. One machine, one thing at a time.
This guarantees reproducible results across different machines.

### Devnet Phases (native, merge, native-p2pool)

| # | Phase | What It Validates |
|---|-------|-------------------|
| 1 | Clean | Docker prune (containers, images, volumes, build cache), kill stale cargo/rustc processes, remove wallet secret |
| 2 | Prerequisites | `dww` binary exists, WASM contracts present (money_v3, DEX, dao_escrow), mode-specific files (Dockerfile.monero, Dockerfile.p2pool, entrypoint scripts) |
| 3 | Wallet | Generate keypair via `dww wallet keygen`, write secret to `/tmp/dwow_mining_secret` |
| 4 | Build | `docker compose --profile <mode> build` for all services in the profile |
| 5 | Start | `docker compose --profile <mode> up -d`, verify no containers exit immediately |
| 6 | Verify containers | Every expected container is running (3 for native, 6 for merge/native-p2pool) |
| 7 | RPC health | TCP JSON-RPC ping to node0 (port 31345) and node1 (31346); plus adaptor RPC (28081) for native-p2pool or monerod RPC (28081) for merge |
| 8 | Mining activity | Log inspection for stratum job acceptance (native), p2pool sidechain activity (merge), or adaptor connectivity (native-p2pool) |
| 9 | Block production | Fetch genesis block via `blockchain.get_block_linear`, wait up to 130s for block height >= 2, inspect PoW data |
| 10 | Report | PASS/FAIL summary printed; exit 0 if all pass, exit 1 with debug instructions if any fail |

### Join Phases (join-native, join-merge)

| # | Phase | What It Validates |
|---|-------|-------------------|
| 1 | Clean | Same as devnet plus removal of join containers (`dwow-test-node`, `dwow-fallback-lilith`), all compose profiles, test data directories |
| 2 | Prerequisites | `join-testnet.sh`, entrypoint.sh, docker-compose.yml, Dockerfile, plus mode-specific files |
| 3 | Wallet | Same as devnet |
| 4 | Build | `docker compose --profile join-merge build` (join-merge) or `--profile native build lilith` (join-native) |
| 5 | Static config | Start container with public testnet params, extract `dwowd_config.toml`, validate 13 config values: magic_bytes, network, seed addresses, hostlist path, localnet=false, inbound listener, threshold=3, pow_target=120, skip_sync, skip_fees, rpc_listen, external_addrs |
| 6 | Container lifecycle | Start container with host networking, verify it survives 10s, check logs for "Starting dwowd", magic bytes, no fatal ERROR lines |
| 7 | Seed fallback | Start local lilith as fallback seed, start dwowd with deliberately unreachable public seeds pointing only to local lilith, verify P2P connection, check hostlist persistence |
| 8 | P2P connectivity | `p2p.info` JSON-RPC (or log-based fallback), wait up to 90s for active sessions or peer connection evidence |
| 9 | Blockchain sync | `blockchain.info` JSON-RPC (or log-based fallback), wait up to 300s for `block_height > 0` |
| 10 | Mining verification | join-native: check stratum/xmrig in logs, wait up to 360s for block height advance. join-merge: start full 4-container compose stack, verify all running, check monerod sync, p2pool activity, and dwowd RPC reachability |
| 11 | Persistence | Start container with host data directory, verify data files created (`hostlist.tsv` or `*.sled`), stop container, verify data survives on host, restart with same data dir |
| 12 | Report | PASS/FAIL summary; exit 0 if all pass, exit 1 with debug instructions if any fail |

### Verified Results

| Mode | PASS | FAIL | Status |
|------|------|------|--------|
| `native` | 18 | 0 | Verified pass |
| `merge` | 23 | 0 | Verified pass |
| `join-native` | 34 | 0 | Verified pass |
| `join-merge` | 42 | 0 | Verified pass |
| `native-p2pool` | — | — | Fix applied: JIT disabled in P2P RandomX VM, adaptor blob layout corrected, PoW validation added to stratum submit. Pending pipeline verification. |

## Contract Tests

Tests the full economic cycle: mine blocks → fund wallet → deploy contracts →
transfer tokens → pay fees. Run after the pipeline passes.

```bash
# Single-contract test (deploy + transfer)
./contrib/docker/darkwow-testnet/contract_test.sh

# Multi-contract test (deploy money_v3, DEX, dao_escrow + transfers)
./contrib/docker/darkwow-testnet/test-contracts.sh --mode native
./contrib/docker/darkwow-testnet/test-contracts.sh --mode merge
```

## Data Persistence

Blockchain data is stored in named Docker volumes by default:

- `lilith_data` — lilith hostlist and datastore
- `node0_data` — node0 blockchain and mining address
- `node1_data` — node1 blockchain and mining address

To persist data outside Docker volumes, mount host directories in
`docker-compose.yml` or use `-v` with `docker run`.

## Networking

The default `docker-compose.yml` uses **bridge networking** with port mapping —
ideal for single-machine local development. Containers communicate via their
service names (`lilith`, `node0`, `node1`) as hostnames.

For multi-machine deployment, switch to **host networking** (`--network=host` or
`network_mode: host` in compose). This means:

- The container shares the host's network stack
- P2P peers see the host's real IP address (essential for LAN discovery)
- No port mapping needed (ports bind directly on the host)

If you need bridge networking for multi-machine, map ports and set
`EXTERNAL_ADDR` to the host's IP with the mapped port.

## Differences from dwow-devnet Docker

| Feature | `dwow-devnet/` | `darkwow-testnet/` |
|---------|----------------|-------------------|
| `localnet` | `false` | `false` |
| Magic bytes | auto-derived | `[68, 82, 75, 87]` ("DRKW") |
| Consensus threshold | 1 | 3 |
| `fixed_difficulty` | 1 | auto-adjusting |
| Block time | 120s | 120s |
| `skip_fees` | `true` | `false` |
| RPC port (node0) | 31345 | 31345 |
| Stratum port (node0) | 31347 | 31347 |
| Configuration | Fully environment-driven | Fully environment-driven |

## Docker Images

| Image | Source | Build Method | Description | Profiles |
|-------|--------|-------------|-------------|----------|
| `darkwow-testnet-lilith` | `Dockerfile` | Source (multi-stage: git clone → cargo build → xmrig download) | Main image: dwowd + lilith + p2pool-adaptor + xmrig binary | native, merge, native-p2pool |
| `darkwow-testnet-node0` | `Dockerfile` | Same as lilith, tagged per service by compose | Duplicate image for node0 (compose requires per-service tags) | native, merge, native-p2pool |
| `darkwow-testnet-node1` | `Dockerfile` | Same as lilith, tagged per service by compose | Duplicate image for node1 | native, merge, native-p2pool |
| `darkwow-testnet-dwowd-join` | `Dockerfile` | Same as lilith, tagged for join service | Main image for the `dwowd-join` service | join-merge |
| `darkwow-testnet-monerod-join` | `Dockerfile.monero` | Pre-built binary from getmonero.org | Monero daemon (offline mode by default for local devnet; public testnet for join) | merge, join-merge |
| `darkwow-testnet-p2pool-join` | `Dockerfile.p2pool` | Pre-built binary from p2pool GitHub releases (v4.14) | p2pool sidechain node with entrypoint scripts for merge and native modes | merge, native-p2pool, join-merge |
| `darkwow-testnet-xmrig-join` | `Dockerfile.xmrig` | Built from source (cmake, no hwloc) | Standalone xmrig miner for pool mining | merge, native-p2pool, join-merge |

All images share the same Ubuntu 24.04 base. The main `Dockerfile` builds three
Rust binaries (`dwowd`, `lilith`, `dwow-p2pool-adaptor`), four WASM contracts
(`deployooor`, `native_token`, `money_v3`, `baccarat`), and downloads a static
xmrig v6.22.2 binary. Lilith, node0, and node1 images are identical (same
Dockerfile); compose tags them separately for service isolation.

## Compose Profiles

| Profile | Services | Networking | Use Case |
|---------|----------|------------|----------|
| `native` | lilith, node0, node1 | Bridge (`dwow-local`) | 3-node local devnet with native RandomX mining (xmrig → dwowd stratum) |
| `merge` | native + monerod, p2pool, xmrig-merge | Bridge (`dwow-local`) | 3-node local devnet with Monero merge mining via p2pool |
| `native-p2pool` | native + adaptor, p2pool-darkwow, xmrig-p2pool | Bridge (`dwow-local`) | 3-node local devnet with adaptor bridging p2pool to dwowd stratum (no Monero) |
| `join-merge` | dwowd-join, monerod-join, p2pool-join, xmrig-join | Host | Single-node merge mining stack joining the public DarkWow testnet |

Services without a `profiles` key in `docker-compose.yml` are always active.
Services with profiles only start when the matching `--profile` flag is passed.
`docker compose --profile native up` starts only the 3 base services;
`docker compose --profile merge up` starts all 6 merge-mining services.
The `join-native` mode does not use compose — it runs a single container via
`docker run --network=host`.

## File Overview

| File | Purpose |
|------|----------|
| `Dockerfile` | Multi-stage build from source (dwowd + lilith + WASM contracts + xmrig) |
| `Dockerfile.monero` | Monero daemon image using pre-built binary from getmonero.org |
| `Dockerfile.p2pool` | p2pool image using pre-built binary (v4.14), with entrypoints for merge and native modes |
| `Dockerfile.xmrig` | Standalone xmrig miner built from source (cmake, no hwloc) |
| `docker-compose.yml` | Service orchestration with 4 profiles (native, merge, native-p2pool, join-merge) |
| `entrypoint.sh` | Dynamic TOML config generation for lilith and dwowd roles; spawns xmrig for native mining |
| `entrypoint-adaptor.sh` | Start dwow-p2pool-adaptor (merge mining protocol bridge) |
| `entrypoint-p2pool.sh` | Start p2pool in merge mining mode (Monero parent chain + DarkWow aux) |
| `entrypoint-p2pool-darkwow.sh` | Start p2pool in native DarkWow mode (adaptor as monerod, DarkWow as sole chain) |
| `entrypoint-monero.sh` | Start monerod for merge mining (offline or connected mode) |
| `build-and-push.sh` | Build and optionally push image to a registry |
| `join-testnet.sh` | Join the public DarkWow testnet as a mining node (native or merge) |
| `test_pipeline.sh` | Single entry point — clean → build → verify across 5 modes, 10-12 phases each |
| `test-contracts.sh` | Multi-contract deploy and transaction test |
| `contract_test.sh` | Single-contract deploy + transfer test |
