# DarkWow Testnet — Containerized Devnet

A 3-node Docker devnet (lilith seed + 2 mining fullnodes) suitable for local
development, multi-machine LAN deployment, and public internet devnets. Magic
bytes `[68, 82, 75, 87]` encode "DRKW" in ASCII, uniquely identifying DarkWow
nodes on the P2P network.

## Quick Start

### Docker Hub (pre-built image)

The Docker image runs the node (dwowd). The wallet (`dwow_wallet`) is a separate native
binary you run on your host — it scans the blockchain, decrypts your coinbase
notes, and shows your balance. Mining rewards are paid to a wallet address you
control.

**Step 1: Build the wallet binary (host)**

```bash
git clone https://codeberg.org/PatrickM123/darkwow.git
cd darkwow
cargo build -p dwow_wallet --release
DRK="./target/release/dwow_wallet"
NETWORK="darkwow-testnet"
```

**Step 2: Generate a keypair for mining rewards**

```bash
$DRK -n $NETWORK wallet keygen
# Output:
#   Address: fao1... (bs58)
#   Secret:  abc123... (hex)
```

**Step 3: Write the secret to a secure file**

```bash
echo -n "<hex-secret>" > /tmp/dwow_mining_secret
chmod 600 /tmp/dwow_mining_secret
```

**Step 4: Start the node**

```bash
docker pull darkrenaissance/darkwow-testnet:latest
docker run -d --name dwow-node --network=host \
  -e ROLE=dwowd \
  -e WALLET_ADDRESS="<bs58-address>" \
  -e WALLET_SECRET_FILE=/run/secrets/mining_secret \
  -e SEED_ADDR=lilith0.dark.fi:31340,lilith1.dark.fi:31340 \
  -e MAGIC_BYTES=68,82,75,87 \
  -v /data/dwowd:/root/.local/share/dwow/dwowd \
  -v /tmp/dwow_mining_secret:/run/secrets/mining_secret:ro \
  darkrenaissance/darkwow-testnet:latest

# Clean up the secret file
rm -f /tmp/dwow_mining_secret
```

`MINING_THREADS` defaults to `1`. Each node mines internally — native
mode uses the built-in RandomX miner via RPC, merge mode runs xmrig as
a sidecar connecting to p2pool. No external miner processes on the host.

| Setting | Threads | Use case |
|---------|---------|----------|
| Default | `1` | Light mining in containers |
| Recommended | `2` | Moderate hashrate |
| Maximum | all physical cores | Dedicated mining |

Pass `-e MINING_THREADS=4` to `docker run` to set a higher count.

**Step 5: Wait for blocks, then collect rewards**

```bash
# Check block height — wait until it advances past the genesis block
curl -s http://127.0.0.1:31345 -X POST \
  -H 'Content-Type: application/json' \
  -d '{"method":"blockchain.info","params":[],"id":1}'

# Scan the blockchain for coins (decrypts your AEAD-encrypted coinbase notes)
$DRK -n $NETWORK scan

# Check your balance
$DRK -n $NETWORK wallet balance
```

Coinbase rewards follow an exponential-decay emission schedule starting at
~13.84 DRKW, with a tail emission floor of ~0.80 DRKW. Total supply cap is
21,000,000 DRKW.

### Build from source (local devnet)

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
# Native mining — node mines internally via RPC miner
./contrib/docker/darkwow-testnet/join-testnet.sh --mode native

# Merge mining — node mines via internal xmrig sidecar → p2pool → Monero + DarkWow
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

All containers communicate over the `dwow-local` Docker bridge network. No ports
are published to the host for mining containers — the pipeline uses `docker exec`
to query RPC from inside containers.

**Native mode** (3 containers):

| Container | Role | P2P Port | RPC Port |
|-----------|------|----------|----------|
| `dwow-lilith` | P2P seed (lilith) | 31340 | — |
| `dwow-node0` | Mining fullnode (dwowd, built-in RPC miner) | 31342 | 31345 |
| `dwow-node1` | Mining fullnode (dwowd, built-in RPC miner) | 31343 | 31346 |

**Merge mode** (6 containers — adds Monero merge mining stack):

| Container | Role | P2P Port | RPC Port |
|-----------|------|----------|----------|
| `dwow-lilith` | P2P seed (lilith) | 31340 | — |
| `dwow-node0` | Merge-mining fullnode (dwowd + xmrig sidecar) | 31342 | 31345 |
| `dwow-node1` | Merge-mining fullnode (dwowd + xmrig sidecar) | 31343 | 31346 |
| `dwow-node2` | Native-mining fullnode (dwowd, built-in RPC miner) | 31344 | 31350 |
| `dwow-monerod` | Monero daemon (synced testnet, offline=false) | — | 28081 |
| `dwow-p2pool` | P2Pool stratum + merge mining bridge | — | 3333 |

Each mining node connects to lilith as its P2P seed and peers with the others.
**Native mode** uses the built-in RPC miner loop (`miner.mine_linear`). **Merge mode**
runs xmrig as a sidecar inside node0 and node1, connecting to p2pool's stratum.
Node2 mines natively in merge mode, providing a competing non-merge miner for
full consensus interaction testing. Coinbase rewards are paid to auto-generated
mining addresses.

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
| `MINING_ENABLED` | `true` | Auto-start mining (native RPC loop or xmrig sidecar) |
| `MINING_THREADS` | `1` | Mining thread count for native RPC or xmrig |
| `RANDOMX_MAX_THREADS` | `0` | Maximum RandomX VM threads (0 = unlimited) |
| `THRESHOLD` | `3` | Confirmation threshold in blocks |
| `SKIP_SYNC` | `false` | Skip blockchain sync on startup |
| `SKIP_FEES` | `false` | Disable fee verification |
| `WALLET_ADDRESS` | auto | Mining payout address (auto-generated if unset) |
| `WALLET_SECRET_FILE` | (empty) | Path to file containing hex-encoded secret key (preferred) |
| `WALLET_SECRET` | auto | Hex-encoded secret key (deprecated — use WALLET_SECRET_FILE) |
| `MERGE_MINING` | `false` | Enable merge mining via Monero p2pool |
| `MM_RPC_PORT` | `31348` | Merge mining JSON-RPC port (p2pool protocol) |
| `FINALITY_MODE` | `always` | Finality mode: `always`, `never`, or `auto` |
| `FINALITY_DISABLE_CARIBINA` | `false` | Disable Caribina finality proofs |
| `FINALITY_ENABLE_MONERO` | `false` | Enable Monero finality anchors (auto-enabled when MERGE_MINING=true) |
| `MONERO_MIN_CONFIRMATIONS` | `3` | Minimum Monero confirmations before accepting anchor |
| `MONEROD_RPC_URL` | (empty) | Monero daemon JSON-RPC URL for finality verification |
| `DATADIR` | `~/.local/share/dwow/dwowd/<network>` | Blockchain data directory |

## Base Image

All DarkWow Docker images inherit from `darkwow-base:24.04` — a pre-baked
Ubuntu 24.04 image with every apt dependency, Rust toolchain, xmrig v6.22.2 (for merge mining sidecar),
and b3sum installed. This base image is built once and reused, saving 5–10
minutes of apt-get + Rust installation per pipeline run. The test pipeline
builds it automatically if missing.

```bash
# Build the base image once:
./contrib/docker/darkwow-testnet/build-base.sh

# Or build and push to a registry:
REGISTRY=docker.io/myuser/ ./contrib/docker/darkwow-testnet/build-base.sh

# Inspect to verify:
docker image inspect darkwow-base:24.04
```

All per-profile Dockerfiles are pure git clone + cargo build with zero
apt-get overhead. Adding new system dependencies to any Dockerfile should
be done in `Dockerfile.base`, then all Dockerfiles pick them up for free.

## Building from Source

With the base image present, building takes 30–60 minutes on a typical machine
(8 GB RAM, 4 cores). Ensure sufficient disk space (~15 GB for build artifacts).

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

## Wallet Docker Container

The wallet container (`darkwow-wallet`) is a standardized, buildable, pushable
Docker image — same pattern as the bridge-node. It builds only `dwow_wallet` (no WASM
contracts, no dwowd, no lilith) and runs in one of two modes:

| Mode | `WALLET_MODE` | Behavior | Use case |
|------|--------------|----------|----------|
| `test` | `test` | Auto-init, scan, run `position`, assert output, exit with status | CI / automated testing |
| `interactive` | `interactive` | Init wallet, then `sleep infinity` for `docker exec` access | Hygienic dev work, high-stakes contract interactions |

The container connects to `node0` RPC via internal Docker DNS (`tcp://node0:31345`)
and shares the same mining secret as the nodes via bind-mount. Your secret key
never leaves the container.

### Build

```bash
# Build wallet image only (~5min vs ~20min for full testnet image)
IMAGE_NAME=darkwow-wallet ./contrib/docker/darkwow-testnet/build-and-push.sh

# Or build directly
docker build -t darkwow-wallet:latest \
  -f contrib/docker/darkwow-testnet/Dockerfile.wallet .
```

### Interactive Mode (Dev Work)

```bash
# Start wallet container alongside the running testnet
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml \
  --profile wallet up -d wallet

# Execute wallet operations inside the container
docker exec dwow-wallet dwow_wallet wallet address
docker exec dwow-wallet dwow_wallet scan
docker exec dwow-wallet dwow_wallet position
docker exec dwow-wallet dwow_wallet wallet balance

# The config is at /root/.config/dwow/drk.toml inside the container.
# To use a different config path:
docker exec dwow-wallet dwow_wallet -c /root/.config/dwow/drk.toml position

# Tear down
docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml \
  --profile wallet down -v
```

### Test Mode (CI / Automated Testing)

```bash
# Run the Level 3 wallet integration test
./contrib/docker/darkwow-testnet/test-wallet.sh

# Or manually:
WALLET_MODE=test docker compose -f contrib/docker/darkwow-testnet/docker-compose.yml \
  --profile wallet up -d wallet

# Container auto-exits after test; check result
docker wait dwow-wallet
docker logs dwow-wallet
```

The test mode entrypoint runs `dwow_wallet wallet init` → `dwow_wallet scan` → `dwow_wallet position`,
asserts on output (coin capabilities, descriptors, actions), and exits 0 on
success or 1 on failure.

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `WALLET_MODE` | `interactive` | `test` (auto-assert, exit) or `interactive` (sleep, docker exec) |
| `WALLET_PASS` | `walletpass` | Wallet database encryption passphrase |
| `WALLET_SECRET` | (empty) | Hex-encoded secret key (prefer `WALLET_SECRET_FILE`) |
| `WALLET_SECRET_FILE` | `/run/secrets/mining_secret` | Path to file containing hex-encoded secret key |

The wallet service is defined in `docker-compose.yml` with `profiles: ["wallet"]`
and starts independently from the native/merge profiles.

## Wallet Setup (Host)

### Pre-Configured Wallet (Recommended)

Generate a keypair on the host and pass it to the container via a file mount.
The miner sends coinbase rewards directly to a wallet you already control —
no secret extraction needed.

```bash
NETWORK="darkwow-testnet"
DRK="./target/release/dwow_wallet"

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
All mining happens inside node containers — no standalone xmrig on the host.

```
MERGE_MINING=false (default — 3 containers)
  node0, node1: dwowd RPC miner (built-in RandomX, in-container)

MERGE_MINING=true (6 containers)
  node0, node1: xmrig sidecar → p2pool:3333 → monerod (parent chain)
                                             → node0:31348 mm_rpc (aux chain)
  node2:        dwowd RPC miner (native, competing with merge miners)
```

### Quick Start

```bash
# Start with merge mining (adds node2 + monerod + p2pool containers; 6 total)
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
| `MONERO_FIXED_DIFFICULTY` | `1000` | Fixed difficulty when offline |
| `MONERO_ADD_PEERS` | (empty) | Comma-separated Monero bootstrap `host:port` |
| `MONERO_WALLET_ADDRESS` | (empty) | Monero wallet for p2pool mining rewards |
| `WALLET_ADDRESS` | (empty) | DarkWow wallet for aux chain mining rewards |

## Test Pipeline

`test_pipeline.sh` is the single entry point for all builds and tests. Every mode
builds Docker images, starts the stack, and runs 10-12 sequential verification
phases. Every check reports PASS or FAIL — there are no skipped or silent checks.

### Modes

| Mode | Type | Profile | Services | Phases | PASS |
|------|------|---------|----------|--------|------|
| `native` | 3-node local devnet | `native` | 3 (lilith, node0, node1) | 10 | 18 |
| `merge` | 3-node local devnet + Monero merge mining | `merge` | 6 (lilith, 3 fullnodes, monerod, p2pool) | 10 | 31 |
| `join-native` | Single node joining public testnet | — (docker run, host net) | 1 | 12 | 34 |
| `join-merge` | Single merge-mining node, public testnet | `join-merge` | 4 | 12 | 42 |

```bash
# Local 3-node devnet (10 phases each)
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode merge

# Join public testnet as a single node (12 phases each)
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode join-native
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode join-merge

# With wallet container (native/merge modes only)
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native --with-wallet
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode merge --with-wallet
```

**Sequential determinism**: Every phase runs to completion before the next begins.
No background tasks, no parallel operations. One machine, one thing at a time.
This guarantees reproducible results across different machines.

### Build Options

| Flag | Default | Effect |
|------|---------|--------|
| `--no-cache` | off | Pass `--no-cache` to every `docker compose build` — all layers rebuilt from scratch |
| `--fresh` | off | Aggressive Phase 1 clean: `docker builder prune -a -f`, `docker rmi -f` for stale images, `docker buildx prune`, `docker volume prune -f` |
| `--with-wallet` | off | Build, start, and verify wallet Docker container alongside devnet (native/merge modes only) |

The flags are independent — combine them for a fully deterministic rebuild:

```bash
# Fast incremental (default) — reuses Docker build cache
./test_pipeline.sh --mode merge

# No-cache rebuild without full clean
./test_pipeline.sh --mode merge --no-cache

# Full clean but cached build
./test_pipeline.sh --mode merge --fresh

# Deterministic: no-cache + full clean
./test_pipeline.sh --mode merge --no-cache --fresh
```

### Devnet Phases (native, merge)

| # | Phase | What It Validates |
|---|-------|-------------------|
| 1 | Clean | Container/volume teardown, kill stale cargo/rustc processes, remove wallet secret. With `--fresh`: also prunes all Docker build cache, images, and buildx builders. |
| 2 | Prerequisites | `dwow_wallet` binary exists, WASM contracts present (promissory_note, DEX, dao_escrow), mode-specific files (Dockerfile.monero, Dockerfile.p2pool, entrypoint scripts) |
| 3 | Wallet | Generate keypair via `dwow_wallet wallet keygen`, write secret to `/tmp/dwow_mining_secret` |
| 4 | Build | `docker compose --profile <mode> build` for all services in the profile. With `--no-cache`: rebuilds all layers from scratch (ensures `git clone` fetches latest). With `--with-wallet`: also builds wallet container (`--profile wallet build`). |
| 5 | Start | `docker compose --profile <mode> up -d`, verify no containers exit immediately. With `--with-wallet`: also starts wallet container (`WALLET_MODE=interactive`). |
| 6 | Verify containers | Every expected container is running (3 for native, 6 for merge). With `--with-wallet`: also expects `dwow-wallet` (+1 container). |
| 7 | RPC health | TCP JSON-RPC ping to node0 (port 31345) and node1 (31346); plus monerod RPC (28081) for merge |
| 8 | Mining activity | Log inspection for stratum job acceptance (native), p2pool sidechain activity + xmrig hashing + merge mining submissions (merge) |
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
| `merge` | 31 | 0 | Verified pass |
| `join-native` | 34 | 0 | Verified pass |
| `join-merge` | 42 | 0 | Verified pass |

## Contract Tests

Tests the full economic cycle: mine blocks → fund wallet → deploy contracts →
transfer tokens → pay fees. Run after the pipeline passes.

```bash
# Single-contract test (deploy + transfer)
./contrib/docker/darkwow-testnet/contract_test.sh

# Multi-contract test (deploy promissory_note, DEX, dao_escrow + transfers)
./contrib/docker/darkwow-testnet/test-contracts.sh --mode native
./contrib/docker/darkwow-testnet/test-contracts.sh --mode merge
```

## Data Persistence

Blockchain data is stored in named Docker volumes by default:

- `lilith_data` — lilith hostlist and datastore
- `node0_data` — node0 blockchain and mining address
- `node1_data` — node1 blockchain and mining address
- `node2_data` — node2 blockchain and mining address (merge profile only)
- `monerod_data` — Monero blockchain (merge mining)
- `p2pool_data` — p2pool sidechain data (merge mining)
- `bridge_node_data` — bridge node state (bridge profile)

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

All images inherit from `darkwow-base:24.04` (the pre-baked system dependency
layer). No Dockerfile runs `apt-get install` — every package is already in the
base image.

| Image | Source | Build Method | Description | Profiles |
|-------|--------|-------------|-------------|----------|
| `darkwow-base:24.04` | `Dockerfile.base` | Pre-baked once (apt + Rust + xmrig + b3sum) | System dependencies, Rust toolchain, xmrig v6.22.2, b3sum | All (build-time dependency) |
| `darkwow-testnet` | `Dockerfile` | Source (git clone → cargo build) | dwowd + lilith + WASM contracts | native, merge, join-merge |
| `darkwow-monerod` | `Dockerfile.monero` | Pre-built binary from getmonero.org (v0.18.5.0, checksum-verified) | Monero daemon | merge, join-merge |
| `darkwow-p2pool` | `Dockerfile.p2pool` | Pre-built binary from p2pool GitHub releases (v4.14, checksum-verified) | p2pool sidechain node | merge, join-merge |
| `darkwow-wallet` | `Dockerfile.wallet` | Source (git clone → cargo build -p dwow_wallet) | Wallet CLI (`dwow_wallet`) for position resolution, scanning, transfers | wallet |

The main `Dockerfile` builds two Rust binaries (`dwowd`, `lilith`) and four
WASM contracts (29 contracts: native_token, promissory_note, DEX, etc). xmrig
is inherited from the base image. Compose tags a per-service copy of each image
for service isolation (e.g. `darkwow-testnet:latest` for `lilith`, `node0`,
`node1`, and `dwowd-join`).

## Compose Profiles

| Profile | Services | Networking | Use Case |
|---------|----------|------------|----------|
| `native` | lilith, node0, node1 | Bridge (`dwow-local`) | 3-node local devnet with native RandomX mining (xmrig → dwowd stratum) |
| `merge` | native + monerod, p2pool | Bridge (`dwow-local`) | 3-node local devnet with Monero merge mining via p2pool |
| `join-merge` | dwowd-join, monerod-join, p2pool-join | Host | Single-node merge mining stack joining the public DarkWow testnet |
| `wallet` | wallet | Bridge (`dwow-local`) | Isolated wallet container for position resolution, scanning, and contract interactions |

Services without a `profiles` key in `docker-compose.yml` are always active.
Services with profiles only start when the matching `--profile` flag is passed.
`docker compose --profile native up` starts only the 3 base services;
`docker compose --profile merge up` starts all 5 merge-mining services.
The `join-native` mode does not use compose — it runs a single container via
`docker run --network=host`.

## File Overview

| File | Purpose |
|------|----------|
| `Dockerfile.base` | **Base image** — all apt packages + Rust toolchain. Built once, inherited by all other Dockerfiles |
| `build-base.sh` | Build and optionally push the base image |
| `Dockerfile` | Multi-stage build from base (git clone + cargo build: dwowd + lilith + WASM) |
| `Dockerfile.monero` | Monero daemon image using pre-built binary from getmonero.org. Inherits from base |
| `Dockerfile.p2pool` | p2pool + xmrig image using pre-built binaries. Inherits from base |
| `docker-compose.yml` | Service orchestration with 3 profiles (native, merge, join-merge) |
| `entrypoint.sh` | Dynamic TOML config generation for lilith and dwowd roles; spawns xmrig for native mining |
| `entrypoint-p2pool.sh` | Start p2pool + xmrig in merge mining mode (Monero parent chain + DarkWow aux) |
| `entrypoint-monero.sh` | Start monerod for merge mining (offline or connected mode) |
| `build-and-push.sh` | Build and optionally push image to a registry |
| `join-testnet.sh` | Join the public DarkWow testnet as a mining node (native or merge) |
| `test_pipeline.sh` | Single entry point — clean → build → verify across 4 modes, 10-12 phases each. Auto-builds base image if missing |
| `test-contracts.sh` | Multi-contract deploy and transaction test |
| `contract_test.sh` | Single-contract deploy + transfer test |
| `Dockerfile.wallet` | Wallet container — builds only `dwow_wallet` (no WASM, no dwowd, no lilith). Fast build (~5min) |
| `entrypoint-wallet.sh` | Wallet entrypoint — generates `drk.toml`, imports/generates keypair, dispatches test/interactive mode |
| `test-wallet.sh` | Level 3 wallet container integration test — starts container in test mode, verifies position output |

## Troubleshooting

### Phantom P2P peer at 172.18.0.1 — stack overflow in sync task

If node0 crashes with `thread '<unknown>' has overflowed its stack` at
`subscribe_msg::<Tip>()`, check for orphaned dwowd/lilith processes running
directly on the Docker host (not in containers):

```bash
ps aux | grep -E '/app/dwowd|/app/lilith|target/.*/dwowd|target/.*/lilith' | grep -v grep
```

These processes connect to the Docker bridge network from the host side,
appearing as `172.18.0.1` — the bridge gateway. The sync task sees them as
peers and tries to query them for block tips. Since they're not full nodes
(or are from a different test run with incompatible state), the interaction
crashes.

Kill them before running the pipeline:
```bash
pkill -9 -f '/app/dwowd'
pkill -9 -f '/app/lilith'
```

The pipeline's Phase 1 (clean) now does this automatically.

## Developer Tools

### Python Dockernet Model

A 1-to-1 Python model of the full dockernet exists at
`contrib/model/dockernet_model.py`. It models two mining nodes producing
blocks continuously with P2P broadcast and fork resolution. No Docker needed.

```bash
python3 contrib/model/dockernet_model.py
```

The model maps every Rust function 1-to-1 and is useful for understanding
block production flow, validating consensus rules, and debugging without
waiting for full Docker builds. See `doc/src/arch/consensus/chain_architecture.md`
for the implementation architecture.
