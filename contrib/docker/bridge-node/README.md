# DarkWow Bridge Node

Single container image for running a cross-chain bridge relayer with capital
endowment. Combines dwowd fullnode, bridge/endowment contract deployment, and
universal_relayer into one image with three runtime modes.

## Base Image

This image inherits from `darkwow-base:24.04` — a pre-baked Ubuntu 24.04 image
with every apt dependency and Rust toolchain across all build profiles. Build
the base image once; all subsequent Docker builds skip system package installation.

```bash
./contrib/docker/darkwow-testnet/build-base.sh
```

## Quick Start

With the base image present:

```bash
# Build the image
docker build -t darkwow-node/bridge . -f contrib/docker/bridge-node/Dockerfile

# Or use the build script
./contrib/docker/bridge-node/build-and-push.sh
```

## Runtime Modes

### MODE=full — All-in-One Bridge Node

Starts dwowd, deploys bridge + relayer_endowment + money_v3 + deployooor contracts,
then starts universal_relayer. Everything runs in one container.

```bash
docker run --network=host \
    -e MODE=full \
    -e NETWORK=darkwow-testnet \
    -e WALLET_SECRET_FILE=/run/secrets/mining_secret \
    -v /path/to/mining_secret:/run/secrets/mining_secret:ro \
    -v /path/to/data:/root/.local/share/dwow/dwowd \
    darkwow-node/bridge:latest
```

### MODE=relayer-only — External dwowd

Runs only universal_relayer, connecting to an existing dwowd instance. Use when
you already have a synced fullnode and want to add bridge relay capabilities.

```bash
docker run --network=host \
    -e MODE=relayer-only \
    -e DARKFID_URL=tcp://192.168.1.100:31345 \
    -e ETH_ENABLED=true \
    -e ETH_NODE_URL=https://mainnet.infura.io/v3/YOUR_KEY \
    -e ETH_RELAYER_PRIVATE_KEY=0x... \
    darkwow-node/bridge:latest
```

### MODE=lilith — P2P Seed Node

Standalone P2P seed for network bootstrapping. No chain state, no contracts.

```bash
docker run --network=host \
    -e MODE=lilith \
    -e NETWORK=darkwow-testnet \
    -e P2P_PORT=31340 \
    darkwow-node/bridge:latest
```

## Docker Compose

### Full Stack (seed + bridge node)

```bash
docker compose -f contrib/docker/bridge-node/docker-compose.yml --profile full up -d
```

### Relayer-Only (external dwowd)

```bash
DARKFID_URL=tcp://192.168.1.100:31345 \
    docker compose -f contrib/docker/bridge-node/docker-compose.yml --profile relayer-only up -d
```

## Environment Variables

### Common

| Variable | Default | Description |
|----------|---------|-------------|
| `MODE` | `full` | `full`, `relayer-only`, or `lilith` |
| `NETWORK` | `darkwow-testnet` | Blockchain network name |
| `P2P_PORT` | `31342` | P2P port |
| `RPC_PORT` | `31345` | JSON-RPC port |
| `SEED_ADDR` | `lilith0.dark.fi:31340,...` | Comma-separated seed list |
| `MAGIC_BYTES` | auto-derived | Network magic bytes |
| `WALLET_SECRET_FILE` | — | Path to mining secret file |

### Bridge Contract

| Variable | Default | Description |
|----------|---------|-------------|
| `BRIDGE_RELAYER_FEE_BP` | `100` | Relayer fee in basis points |
| `BRIDGE_TIMEOUT_BLOCKS` | `100` | Withdrawal timeout in blocks |

### Chain Enable Flags

| Variable | Default | Description |
|----------|---------|-------------|
| `ETH_ENABLED` | `false` | Enable Ethereum bridge |
| `XMR_ENABLED` | `false` | Enable Monero bridge |
| `ZEC_ENABLED` | `false` | Enable Zcash bridge |
| `AZT_ENABLED` | `false` | Enable Aztec bridge |
| `LTC_ENABLED` | `false` | Enable Litecoin bridge |

### Universal Relayer

| Variable | Default | Description |
|----------|---------|-------------|
| `DARKFID_URL` | `tcp://127.0.0.1:31345` | dwowd RPC endpoint |
| `POLL_INTERVAL_SECS` | `10` | Withdrawal poll interval |
| `MAX_CONCURRENT_WITHDRAWALS` | `10` | Max parallel withdrawals |
| `RELAYER_TIMEOUT_BLOCKS` | `100` | Blocks before cancellation |
| `RELAYER_FEE_PERCENTAGE` | `1` | Relayer fee percentage |

### Per-Chain Configuration

Each chain has specific env vars for node URLs, keys, and confirmations.
See the [universal_relayer config](../../../bin/universal_relayer/universal_relayer_config.toml)
for the full template. Set chain-specific vars at runtime:

```bash
# Ethereum
ETH_NODE_URL=https://mainnet.infura.io/v3/YOUR_KEY
ETH_RELAYER_PRIVATE_KEY=0x...
ETH_MAX_GAS_GWEI=50

# Monero
XMR_WALLET_RPC_URL=http://127.0.0.1:18083
XMR_NODE_RPC_URL=http://127.0.0.1:18081

# Zcash
ZEC_NODE_RPC_URL=http://127.0.0.1:8232

# Litecoin
LTC_NODE_RPC_URL=http://127.0.0.1:9332
LTC_RPC_USER=user
LTC_RPC_PASS=pass

# Aztec
AZT_ROLLUP_ADDRESS=0x...
AZT_SEQUENCER_URL=https://aztec.network
```

## Architecture

```
┌──────────────────────────────────────────────────┐
│              bridge-node (MODE=full)              │
│                                                   │
│  ┌─────────┐  ┌──────────────────┐               │
│  │ dwowd   │  │ universal_relayer│               │
│  │ fullnode│──│  - eth executor  │               │
│  │         │  │  - xmr executor  │               │
│  │ bridge  │  │  - zec executor  │               │
│  │ endowment│  │  - ltc executor  │               │
│  │ money_v3│  │  - azt executor  │               │
│  └─────────┘  └──────────────────┘               │
│                                                   │
│  Bridge contract receives deposits, relayer       │
│  executes withdrawals on external chains,         │
│  endowment distributes fees to capital backers.   │
└──────────────────────────────────────────────────┘
```

## Contract Deployment Order

The full mode deploys contracts in dependency order:

1. **deployooor** — Contract factory
2. **money_v3** — Token layer (required by bridge)
3. **bridge** — Cross-chain bridge (initialized with fee + timeout)
4. **relayer_endowment** — Capital endowment (initialized with backer cut)

## Verification

```bash
# Check bridge node status
docker exec <container> /app/universal_relayer --config /root/.config/dwow/universal_relayer.toml status

# Check RPC health
docker exec <container> bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"params\":[],\"id\":1}" >&3; timeout 3 cat <&3'

# View logs
docker logs -f <container>

# List deployed contracts (from inside container)
docker exec <container> /app/dwow_wallet -n darkwow-testnet contract list
```

## Volumes

| Path | Contents |
|------|----------|
| `/root/.local/share/dwow/dwowd/` | Blockchain database, mining keys |
| `/root/.local/share/dwow/drk/` | Wallet database |
| `/root/.local/share/dwow/lilith/` | Lilith seed data |

## Security

All contracts are **EXPERIMENTAL** and **UNAUDITED**. The bridge node handles
real assets — use at your own risk. Store secrets via `WALLET_SECRET_FILE` mounted
as a Docker secret, never as environment variables in production.
