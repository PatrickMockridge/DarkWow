# DarkWow Public Testnet Node

Single container image for joining the public DarkWow testnet as a mining node.
One image, two mining modes, zero external dependencies beyond Docker.

## Quick Start

```bash
# Pull from Docker Hub
docker pull darkwow-node/testnet:latest

# Native mining (solo RandomX)
docker run --network=host \
    -e MODE=native \
    -e WALLET_SECRET_FILE=/run/secrets/mining_secret \
    -v /path/to/mining_secret:/run/secrets/mining_secret:ro \
    -v /path/to/data:/root/.local/share/dwow/dwowd \
    darkwow-node/testnet:latest

# Merge mining (Monero p2pool)
docker run --network=host \
    -e MODE=merge \
    -e WALLET_SECRET_FILE=/run/secrets/mining_secret \
    -v /path/to/mining_secret:/run/secrets/mining_secret:ro \
    -v /path/to/data:/root/.local/share/dwow/dwowd \
    darkwow-node/testnet:latest
```

`--network=host` is required — the P2P layer needs the host's real IP for peer discovery.

## What's Inside

| Binary | Version | Purpose |
|--------|---------|---------|
| `dwowd` | 0.5.0 (from source) | DarkWow fullnode |
| `lilith` | 0.5.0 (from source) | P2P seed/bootstrap node |
| `xmrig` | 6.22.2 (static) | RandomX CPU miner |
| `monerod` | latest (pre-built) | Monero daemon (merge mining) |
| `p2pool` | 4.14 (pre-built) | P2Pool sidechain (merge mining) |

## Modes

### `MODE=native` (default)

Solo RandomX mining. dwowd connects to public testnet seeds, xmrig mines via
dwowd's built-in stratum server.

```
xmrig → dwowd stratum → public testnet seeds → blockchain
```

### `MODE=merge`

Monero merge mining via p2pool. xmrig mines on p2pool, which submits blocks to
both Monero testnet (parent chain) and DarkWow (aux chain via dwowd mm_rpc).

```
xmrig → p2pool → monerod (parent)
              → dwowd mm_rpc (aux)
```

DarkWow borrows security from Monero's hashpower.

### `MODE=lilith`

Standalone P2P seed node. For operators contributing to testnet infrastructure.

## Environment Variables

### All modes

| Variable | Default | Description |
|----------|---------|-------------|
| `MODE` | `native` | `native`, `merge`, or `lilith` |
| `NETWORK` | `darkwow-testnet` | Network identifier |
| `P2P_PORT` | `31342` | P2P inbound port |
| `MAGIC_BYTES` | auto-derived | Network magic bytes |
| `LOCALNET` | `false` | Disable TLS cert validation |

### MODE=native

| Variable | Default | Description |
|----------|---------|-------------|
| `RPC_PORT` | `31345` | JSON-RPC port |
| `STRATUM_PORT` | `31347` | Stratum mining port |
| `SEED_ADDR` | `lilith0.dark.fi:31340,lilith1.dark.fi:31340` | Comma-separated seed list |
| `EXTERNAL_ADDR` | (auto) | Public host:port for P2P |
| `THRESHOLD` | `3` | Consensus threshold |
| `TARGET_BLOCK_TIME` | `120` | Block time target (seconds) |
| `MINING_THREADS` | `1` | xmrig thread count |
| `WALLET_ADDRESS` | (auto-generated) | Mining reward address |
| `WALLET_SECRET` | — | Hex secret key (env var) |
| `WALLET_SECRET_FILE` | — | Path to file containing secret (preferred) |

### MODE=merge (all native vars plus)

| Variable | Default | Description |
|----------|---------|-------------|
| `MONERO_OFFLINE` | `false` | Run monerod offline |
| `MONERO_NETWORK` | `testnet` | Monero network (`testnet` or `mainnet`) |
| `MONERO_RPC_PORT` | `28081` | monerod RPC port |
| `MONERO_ZMQ_PORT` | `28083` | monerod ZMQ pub port |
| `MONERO_FIXED_DIFFICULTY` | `20000` | Difficulty in offline mode |
| `MONERO_ADD_PEERS` | (public testnet seeds) | monerod bootstrap peers |
| `MM_RPC_PORT` | `31348` | dwowd merge mining RPC |
| `P2POOL_STRATUM_PORT` | `3333` | p2pool stratum for xmrig |
| `XMERGE_THREADS` | `2` | xmrig thread count for merge |
| `MONERO_WALLET_ADDRESS` | (dummy) | Monero wallet for XMR rewards |

## Base Image

This image inherits from `darkwow-base:24.04` — a pre-baked Ubuntu 24.04 image
with every apt dependency and Rust toolchain across all build profiles. Build
the base image once; all subsequent builds skip system package installation.

```bash
./contrib/docker/darkwow-testnet/build-base.sh
```

## Building from Source

```bash
# Build
./contrib/docker/testnet-node/build-and-push.sh

# Build and push to Docker Hub
REGISTRY=docker.io/darkwow-node/ VERSION=0.1.0 ./contrib/docker/testnet-node/build-and-push.sh
```

## Wallet Setup

Generate a wallet with `dww` before starting the node:

```bash
# Build from source first (`make`), then:
./target/release/dww -n darkwow-testnet wallet keygen
./target/release/dww -n darkwow-testnet wallet address

# Write the secret to a file (never pass raw hex in CLI)
echo "<hex_secret>" > /tmp/mining_secret
chmod 600 /tmp/mining_secret

# Pass to container
docker run ... -e WALLET_SECRET_FILE=/run/secrets/mining_secret \
    -v /tmp/mining_secret:/run/secrets/mining_secret:ro ...
```

The mining address persists in the data volume — on restart, the same keypair is reused.

## Data Persistence

Bind-mount a host directory to persist blockchain data across container restarts:

```bash
-v /srv/dwow-data:/root/.local/share/dwow/dwowd
```

Without this, all chain data is lost when the container is removed.

## Checking Status

```bash
# dwowd RPC
curl -s http://127.0.0.1:31345 -X POST \
    -H 'Content-Type: application/json' \
    -d '{"method":"blockchain.info","params":[],"id":1}'

# P2P connections
curl -s http://127.0.0.1:31345 -X POST \
    -H 'Content-Type: application/json' \
    -d '{"method":"p2p.info","params":[],"id":1}'

# p2pool stats (merge mode)
curl -s http://127.0.0.1:3333/stats
```

## Known Limitations

- **Host networking required.** The P2P layer needs the host's real IP address
  for peer advertisement and NAT traversal. Bridge networking breaks peer discovery.
- **Merge mode is all-in-one.** monerod, dwowd, p2pool, and xmrig all run inside
  a single container. For production multi-machine deployments, run separate
  containers for each service (see the [darkwow-testnet docs](../darkwow-testnet/README.md)).
- **No ZK contract support.** This is a mining node only. Contract deployment
  and interaction requires the full `dww` CLI tool.
- **Security.** `WALLET_SECRET` from env vars is visible in `docker inspect`.
  Always use `WALLET_SECRET_FILE` for production.

## Testing

Unit tests for the entrypoint script cover config generation, wallet preseed,
magic bytes derivation, and error handling. No Docker or binaries required.

```bash
bash contrib/docker/testnet-node/test_entrypoint.sh
```

## Docs

- [Full DarkWow Documentation](../../doc/src/dev/)
- [Testing Infrastructure](../../doc/src/dev/testing/overview.md)
- [Level 4: Containerized Devnet](../../doc/src/dev/testing/level-4-devnet.md)
- [darkwow-testnet Pipeline](../darkwow-testnet/README.md)
