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
3. **External participants** connect with:
   ```bash
   docker run --network=host \
     -e SEED_ADDR=<your-public-ip>:31340 \
     darkwow-testnet:latest
   ```

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
| `SEED_ADDR` | (empty) | `host:port` of seed node for P2P bootstrap |
| `PEER_ADDR` | (empty) | Comma-separated additional peer `host:port` |
| `EXTERNAL_ADDR` | (empty) | Public `host:port` for internet-facing nodes |
| `IS_SEED` | `false` | Run as seed (no upstream seeds configured) |
| `FIXED_DIFFICULTY` | (empty) | Fixed PoW difficulty (unset for auto-adjusting) |
| `TARGET_BLOCK_TIME` | `120` | Block time target in seconds |
| `MINING_ENABLED` | `true` | Auto-start xmrig mining |
| `MINING_THREADS` | `1` | xmrig thread count |
| `THRESHOLD` | `3` | Confirmation threshold in blocks |
| `SKIP_SYNC` | `false` | Skip blockchain sync on startup |
| `SKIP_FEES` | `false` | Disable fee verification |
| `WALLET_ADDRESS` | auto | Mining payout address (auto-generated if unset) |
| `WALLET_SECRET` | auto | Hex-encoded secret key for pre-configured wallet |
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

Generate a keypair on the host and pass it to the container. The miner sends
coinbase rewards directly to a wallet you already control — no manual secret
extraction needed.

```bash
NETWORK="darkwow-testnet"
DRK="./target/release/dww"

# Generate a new keypair for mining rewards
$DRK -n $NETWORK wallet keygen
# Output: address (bs58) and secret (hex)

# Start the testnet with the pre-configured wallet
WALLET_ADDRESS="<bs58-address>"
WALLET_SECRET="<hex-secret>"

docker run --rm --network=host \
  -e ROLE=dwowd \
  -e NETWORK=darkwow-testnet \
  -e WALLET_ADDRESS="$WALLET_ADDRESS" \
  -e WALLET_SECRET="$WALLET_SECRET" \
  -e SEED_ADDR=<seed-host>:31340 \
  -e MAGIC_BYTES=68,82,75,87 \
  -e MINING_THREADS=4 \
  -v /data/node0:/root/.local/share/dwow/dwowd \
  darkwow-testnet:latest

# The wallet already has the key — just scan for coins
$DRK -n $NETWORK scan
$DRK -n $NETWORK wallet balance
```

### Extract from Running Container

If no WALLET_SECRET was provided, the daemon auto-generates a keypair on first
startup. Extract it manually:
# Set network
NETWORK="darkwow-testnet"
DRK="./target/release/dww"

# Extract the mining secret from the running node0
SECRET_HEX=$(docker exec dwow-node0 \
    cat /root/.local/share/dwow/dwowd/darkwow-testnet/mining_secret)

# Initialize wallet and import the mining key
$DRK -n $NETWORK wallet initialize
$DRK -n $NETWORK wallet import-secret-hex "$SECRET_HEX"

# Scan the blockchain for coins
$DRK -n $NETWORK scan

# Check balance (should show DRKW from mining rewards)
$DRK -n $NETWORK wallet balance
```

## Contract Tests

Tests the full economic cycle: mine blocks → fund wallet → deploy contracts →
transfer tokens → pay fees.

```bash
# Full pipeline (clean → build → start → health-check)
./contrib/docker/darkwow-testnet/test_pipeline.sh

# Single-contract test (deploy + transfer)
./contrib/docker/darkwow-testnet/contract_test.sh

# Multi-contract test (deploy money_v3, DEX, dao_escrow + transfers)
./contrib/docker/darkwow-testnet/test-contracts.sh
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

## Differences from linear-testnet Docker

| Feature | `linear-testnet/` | `darkwow-testnet/` |
|---------|-------------------|-------------------|
| `localnet` | `true` | `false` |
| Magic bytes | `[163, 139, 113, 101]` | `[68, 82, 75, 87]` ("DRKW") |
| Consensus threshold | 1 | 3 |
| `pow_target` | 1 | 120 |
| Block time | 60s | 120s |
| `skip_fees` | `false` | `false` |
| RPC port (node0) | 28345 | 31345 |
| Stratum port (node0) | 48347 | 31347 |
| Configuration | Hardcoded per-hostname | Fully environment-driven |

## File Overview

| File | Purpose |
|------|----------|
| `Dockerfile` | Multi-stage build from local source (dwowd + lilith + WASM contracts) |
| `docker-compose.yml` | 3-container orchestration with environment-driven config |
| `entrypoint.sh` | Dynamic config generation for lilith and dwowd roles |
| `build-and-push.sh` | Build and optionally push image to a registry |
| `test_pipeline.sh` | Clean → validate → build → start → health-check |
| `test-contracts.sh` | Multi-contract deploy and transaction test |
| `contract_test.sh` | Single-contract deploy + transfer test |
