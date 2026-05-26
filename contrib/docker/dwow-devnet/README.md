# DarkWow Devnet — Docker Mining Node

A self-contained Docker image that turns any Linux machine into a DarkWow devnet
mining node. Run a private blockchain across idle machines on your LAN, or spin
up a 3-node local devnet on a single machine for development and testing.

## Quick Start

### Local Devnet (single machine, bridge networking)

```bash
# Full pipeline: build, start, health-check
./contrib/docker/dwow-devnet/test_pipeline.sh

# Or manually:
docker compose -f contrib/docker/dwow-devnet/docker-compose.yml up -d

# After blocks are mined, run contract tests:
./contrib/docker/dwow-devnet/contract_test.sh
./contrib/docker/dwow-devnet/test-contracts.sh
```

### Multi-Machine LAN (host networking)

**Machine 1 — Seed:**
```bash
docker run --rm --network=host \
  -e IS_SEED=true \
  -e NETWORK_NAME=our-devnet \
  dwow-devnet:latest
```

**Machine 2+ — Miners:**
```bash
docker run --rm --network=host \
  -e SEED_ADDR=<seed-lan-ip>:31342 \
  -e NETWORK_NAME=our-devnet \
  dwow-devnet:latest
```

This starts a devnet with mining enabled. Blocks mine instantly
(fixed difficulty 1). RPC available on port 31345, stratum mining on 31347.

## Check That It Works

From any machine with the `dwow_wallet` CLI wallet:

```bash
dwow_wallet -n dwow-devnet scan
dwow_wallet -n dwow-devnet wallet balance
```

## Architecture

| Container | Role | P2P Port | RPC Port | Stratum Port |
|-----------|------|----------|----------|--------------|
| `dwow-devnet-lilith` | P2P seed (lilith) | 31340 | — | — |
| `dwow-devnet-node0` | Mining fullnode | 31342 | 31345 | 31347 |
| `dwow-devnet-node1` | Mining fullnode | 31343 | 31346 | 31348 |

Nodes connect to lilith as their P2P seed, plus each other as direct peers.
xmrig mines via local stratum (RandomX `rx/0`), and coinbase rewards are paid
to the configured wallet address.

## Deployment Modes

### Bridge Mode (default)

3 containers on a Docker bridge network (`dwow-devnet-bridge`). Ideal for
single-machine development and CI. DNS-based service discovery — containers
reach each other by service name (`lilith`, `node0`, `node1`).

```bash
docker compose -f contrib/docker/dwow-devnet/docker-compose.yml up -d
```

### Host Mode

2 containers using `network_mode: host`. For multi-machine LAN deployment
where each machine runs one container and P2P peers need the host's real IP.

```bash
docker compose -f contrib/docker/dwow-devnet/docker-compose.yml --profile host up
```

On separate machines, run one service each:

```bash
# Machine 1
docker compose -f contrib/docker/dwow-devnet/docker-compose.yml --profile host up seed

# Machine 2 (edit SEED_ADDR in compose file first)
docker compose -f contrib/docker/dwow-devnet/docker-compose.yml --profile host up miner
```

## Environment Variable Reference

| Variable | Default | Description |
|---|---|---|
| `ROLE` | `dwowd` | `lilith` (P2P seed) or `dwowd` (fullnode) |
| `NETWORK_NAME` | `dwow-devnet` | Unique devnet name (determines P2P isolation) |
| `IS_SEED` | `false` | First node in a fresh devnet (no upstream seeds) |
| `SEED_ADDR` | (empty) | `host:port` of seed to join an existing devnet |
| `EXTERNAL_ADDR` | (empty) | Public `host:port` for internet-facing nodes |
| `MAGIC_BYTES` | auto | 4 comma-separated bytes for P2P isolation |
| `P2P_PORT` | `31342` | P2P inbound listen port |
| `RPC_PORT` | `31345` | JSON-RPC port |
| `STRATUM_PORT` | `31347` | Stratum mining port |
| `MANAGEMENT_PORT` | `31346` | Management RPC port |
| `FIXED_DIFFICULTY` | `1` | Fixed PoW difficulty (unset for dynamic) |
| `TARGET_BLOCK_TIME` | `120` | Block time target in seconds |
| `MINING_ENABLED` | `true` | Auto-start xmrig mining |
| `MINING_THREADS` | `1` | xmrig thread count |
| `RANDOMX_MAX_THREADS` | `0` | Maximum RandomX VM threads (0 = unlimited) |
| `THRESHOLD` | `1` | Confirmation threshold in blocks |
| `SKIP_SYNC` | `true` | Skip blockchain sync on startup |
| `SKIP_FEES` | `true` | Disable fee verification |
| `LOCALNET` | `false` | P2P localnet flag |
| `WALLET_ADDRESS` | auto | Mining payout address (auto-generated if unset) |
| `WALLET_SECRET_FILE` | (empty) | Path to file containing hex-encoded secret key (preferred) |
| `WALLET_SECRET` | auto | Hex-encoded secret key (deprecated — use WALLET_SECRET_FILE) |

## Wallet Setup

### Pre-Configured Wallet (Recommended)

Generate a keypair on the host and pass it to the container via a file mount.
The miner sends coinbase rewards directly to a wallet you already control —
no secret extraction needed.

```bash
NETWORK="dwow-devnet"
DRK="./target/release/dwow_wallet"

# Generate a keypair
$DRK -n $NETWORK wallet keygen
# Output: address (bs58) and secret (hex)

# Write the secret to a secure temp file
echo -n "<hex-secret>" > /tmp/dwow_mining_secret
chmod 600 /tmp/dwow_mining_secret

# Start the devnet — mount the secret file, pass path via env var
docker run --rm --network=host \
  -e IS_SEED=true \
  -e NETWORK_NAME=dwow-devnet \
  -e WALLET_ADDRESS="<bs58-address>" \
  -e WALLET_SECRET_FILE=/run/secrets/mining_secret \
  -v /tmp/dwow_mining_secret:/run/secrets/mining_secret:ro \
  dwow-devnet:latest

# Clean up the temp file
rm -f /tmp/dwow_mining_secret

# The wallet already has the key — scan for coins
$DRK -n $NETWORK scan
$DRK -n $NETWORK wallet balance
```

With docker compose, use the `WALLET_SECRET_FILE` env var (the compose file
passes it through to the container):

```bash
echo -n "<hex-secret>" > /tmp/dwow_mining_secret
chmod 600 /tmp/dwow_mining_secret
WALLET_SECRET_FILE=/tmp/dwow_mining_secret \
  WALLET_ADDRESS="<bs58-address>" \
  docker compose up -d
rm -f /tmp/dwow_mining_secret
```

### Auto-Generated Keypair

If no `WALLET_SECRET` or `WALLET_SECRET_FILE` is provided, dwowd auto-generates
a random keypair on first startup. The secret exists only inside the container's
datadir. Mining rewards are unspendable until the secret is imported into a wallet.

For production: always pre-generate a keypair and provision it securely (SSH,
config management, or mounted secrets file). The `docker exec cat mining_secret`
pattern is **not recommended** — it exposes the secret in the shell history and
treats the container filesystem as a secrets store.

## Test Pipeline

The test pipeline automates build → start → health-check for quick iteration:

```bash
./contrib/docker/dwow-devnet/test_pipeline.sh
```

Phases:
1. Clean previous deployment
2. Validate prerequisites (dwow_wallet binary, Docker files)
3. Generate wallet keypair (stored in local wallet DB)
4. Build Docker images
5. Start containers (bridge mode)
6. Verify containers are running
7. Verify RPC health and mining activity
8. Verify block production

After the pipeline completes, run contract tests:

```bash
./contrib/docker/dwow-devnet/contract_test.sh      # Single contract
./contrib/docker/dwow-devnet/test-contracts.sh     # Multi-contract suite
```

Both scripts use the wallet initialized by `test_pipeline.sh`. They never
extract secrets from containers — the wallet already has the mining key from
step 3 of the pipeline. `dwow_wallet scan` decrypts coinbase notes client-side via
AEAD, following the privacy model.

## Opening to the Internet

To allow external participants (outside your LAN) to join:

1. **On the seed machine**: set up port forwarding on your router for the P2P
   port (default 31342).
2. **Set `EXTERNAL_ADDR`** on the seed:
   ```bash
   docker run --network=host \
     -e IS_SEED=true \
     -e EXTERNAL_ADDR=<your-public-ip>:31342 \
     dwow-devnet:latest
   ```
3. **External participants** connect with:
   ```bash
   docker run --network=host \
     -e SEED_ADDR=<your-public-ip>:31342 \
     dwow-devnet:latest
   ```

## Networking

The container supports two networking modes:

**Bridge networking** (default compose profile): containers communicate via
a Docker bridge network with DNS-based service discovery. Ports are mapped to
the host. Ideal for single-machine development.

**Host networking** (`--profile host` or `--network=host`): the container
shares the host's network stack. P2P peers see the host's real IP address
(essential for LAN discovery). No port mapping needed.

## Base Image

This image inherits from `darkwow-base:24.04` — a pre-baked Ubuntu 24.04 image
with every apt dependency and Rust toolchain across all profiles. The base image
is built once and reused by all DarkWow Dockerfiles, so per-commit builds only
pay for git clone + cargo compile. The test pipeline builds it automatically if
missing.

```bash
# Build the base image once:
./contrib/docker/darkwow-testnet/build-base.sh

# Verify:
docker image inspect darkwow-base:24.04
```

## Building from Source

With the base image present, building takes 30–60 minutes on a typical machine
(8 GB RAM, 4 cores). Ensure sufficient disk space (~15 GB for build artifacts).

```bash
# From the repo root:
docker build -t dwow-devnet . -f contrib/docker/dwow-devnet/Dockerfile
```

Or use the build script:

```bash
./contrib/docker/dwow-devnet/build-and-push.sh
```

## Data Persistence

Blockchain data is stored in named Docker volumes by default:
- `lilith_data` — lilith hostlist and datastore
- `node0_data` — node0 blockchain and mining keys
- `node1_data` — node1 blockchain and mining keys

For host mode, mount a volume directly:

```bash
docker run --network=host \
  -v /data/dwow-devnet:/root/.local/share/dwow/dwowd \
  -e IS_SEED=true \
  dwow-devnet:latest
```

## File Overview

| File | Purpose |
|------|---------|
| `Dockerfile` | Multi-stage build from base (COPY local source + cargo: zkas + WASM + dwowd + lilith + xmrig) |
| `docker-compose.yml` | Bridge (3-node) and host (multi-machine) deployment profiles |
| `entrypoint.sh` | Dynamic config generation for lilith and dwowd roles |
| `build-and-push.sh` | Build and optionally push image to a registry |
| `test_pipeline.sh` | Clean → validate → build → start → health-check pipeline. Auto-builds base image if missing |
| `contract_test.sh` | Single-contract deploy + transfer test |
| `test-contracts.sh` | Multi-contract test (money_v3, DEX, dao_escrow) |

The base image `darkwow-base:24.04` lives in `contrib/docker/darkwow-testnet/Dockerfile.base`
and is shared by this devnet image and all other DarkWow Docker images.
