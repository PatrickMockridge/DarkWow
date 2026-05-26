# Bridge Node — Cross-Chain Relayer Infrastructure

Run a DarkWow bridge relayer with capital endowment in a single Docker container.
The bridge node combines the dwowd fullnode, bridge + relayer_endowment smart
contracts, and universal_relayer into one deployable image.

## Architecture

```
┌───────────────────────────────────────────────────────┐
│                   bridge-node                          │
│                                                       │
│  ┌──────────┐  ┌──────────────────┐                   │
│  │  dwowd   │  │ universal_relayer│                   │
│  │ fullnode │──│                  │                   │
│  │          │  │ Chain executors: │                   │
│  │ contracts│  │  ETH  XMR  ZEC   │── External RPCs   │
│  │  bridge  │  │  LTC  AZT        │                   │
│  │  endow.  │  │                  │                   │
│  │  money   │  │ Stake / Pool     │                   │
│  └──────────┘  └──────────────────┘                   │
│                                                       │
│  Bridge:   deposits/withdrawals for 5 chains          │
│  Endowment: backer capital → relayer fee distribution │
│  Relayer:  polls bridge, executes on external chains  │
└───────────────────────────────────────────────────────┘
```

## Prerequisites

- **Docker** (24.0+) with BuildKit enabled
- **Host networking** — the bridge node needs direct network access for P2P and external chain RPCs
- **External chain nodes** — for each chain you enable, you need a synced RPC endpoint:
  - Ethereum: Infura/Alchemy/self-hosted geth
  - Monero: monerod + monero-wallet-rpc
  - Zcash: zcashd
  - Litecoin: litecoind
  - Aztec: Aztec sequencer endpoint
- **DarkWow wallet** — optional, for pre-seeding mining keys via `WALLET_SECRET_FILE`

## Quick Start

### Full Mode (All-in-One)

```bash
# Build
docker build -t darkwow-node/bridge . -f contrib/docker/bridge-node/Dockerfile

# Run — starts dwowd, deploys contracts, starts relayer
docker run --network=host \
    -e MODE=full \
    -e NETWORK=darkwow-testnet \
    -v /path/to/data:/root/.local/share/dwow/dwowd \
    darkwow-node/bridge:latest
```

### Relayer-Only (External dwowd)

```bash
docker run --network=host \
    -e MODE=relayer-only \
    -e DARKFID_URL=tcp://192.168.1.100:31345 \
    -e ETH_ENABLED=true \
    -e ETH_NODE_URL=https://mainnet.infura.io/v3/YOUR_KEY \
    -e ETH_RELAYER_PRIVATE_KEY=0x... \
    darkwow-node/bridge:latest
```

### Docker Compose

```bash
# Full stack
docker compose -f contrib/docker/bridge-node/docker-compose.yml --profile full up -d

# Relayer-only
DARKFID_URL=tcp://192.168.1.100:31345 \
    docker compose -f contrib/docker/bridge-node/docker-compose.yml --profile relayer-only up -d
```

## Configuration

### Mode Selection

| Mode | What Runs | Use Case |
|------|-----------|----------|
| `full` | dwowd + deploy contracts + universal_relayer | New bridge node from scratch |
| `relayer-only` | universal_relayer only | Add relay to existing fullnode |
| `lilith` | P2P seed node | Network bootstrapping |

### Bridge Contract Settings

The bridge contract handles cross-chain deposits and withdrawals with HTLC
atomic swaps and guaranteed withdrawal execution.

| Parameter | Default | Description |
|-----------|---------|-------------|
| `BRIDGE_RELAYER_FEE_BP` | `100` | Relayer fee in basis points (1%) |
| `BRIDGE_TIMEOUT_BLOCKS` | `100` | Blocks before withdrawal can be cancelled |

### Chain Configuration

Each of the 5 supported chains can be independently enabled. For each enabled
chain, provide the node URL and credentials.

**Ethereum:**
```bash
ETH_ENABLED=true
ETH_NODE_URL=https://mainnet.infura.io/v3/YOUR_KEY
ETH_RELAYER_PRIVATE_KEY=0x...
ETH_MAX_GAS_GWEI=50
```

**Monero:**
```bash
XMR_ENABLED=true
XMR_WALLET_RPC_URL=http://127.0.0.1:18083
XMR_NODE_RPC_URL=http://127.0.0.1:18081
```

**Zcash:**
```bash
ZEC_ENABLED=true
ZEC_NODE_RPC_URL=http://127.0.0.1:8232
```

**Litecoin:**
```bash
LTC_ENABLED=true
LTC_NODE_RPC_URL=http://127.0.0.1:9332
LTC_RPC_USER=user
LTC_RPC_PASS=pass
```

**Aztec:**
```bash
AZT_ENABLED=true
AZT_ROLLUP_ADDRESS=0x...
AZT_SEQUENCER_URL=https://aztec.network
```

### Relayer Settings

| Parameter | Default | Description |
|-----------|---------|-------------|
| `DARKFID_URL` | `tcp://127.0.0.1:31345` | dwowd JSON-RPC endpoint |
| `POLL_INTERVAL_SECS` | `10` | How often to check for withdrawals |
| `MAX_CONCURRENT_WITHDRAWALS` | `10` | Max parallel withdrawal executions |
| `RELAYER_TIMEOUT_BLOCKS` | `100` | Blocks before withdrawal can be cancelled |
| `RELAYER_FEE_PERCENTAGE` | `1` | Relayer fee (percent) |

## Contract Lifecycle

The `full` mode automatically handles contract deployment in dependency order:

1. **deployooor** (factory)
2. **money_v3** (token layer)
3. **bridge** — initialized with `relayer_fee_bp` and `timeout_blocks`
4. **relayer_endowment** — initialized with `default_backer_cut_bp=500` (5%)

After deployment, contracts are registered in the wallet for subsequent
interaction.

### Manual Contract Interaction

```bash
# List deployed contracts
docker exec <container> /app/dwow_wallet -n darkwow-testnet contract list

# Check bridge config
docker exec <container> /app/dwow_wallet -n darkwow-testnet contract invoke <bridge_id> get_config

# Deploy capital to a relayer
docker exec <container> /app/dwow_wallet -n darkwow-testnet contract invoke <endowment_id> deploy_capital \
    --params '{"relayer_pub":"...","amount":1000000,"backer_cut_bp":500}'
```

## Relayer Endowment Flow

The relayer_endowment contract enables capital providers ("backers") to deploy
capital to relayers in exchange for a share of bridge fees.

```
1. Relayer initializes endowment account
2. Backer deploys capital → EndowmentDeployment created
3. Relayer earns bridge fees → calls SettleFees with per-deployment allocations
4. Backer claims accumulated fees via ClaimRelayerFees
5. Backer withdraws principal + remaining fees via WithdrawDeployment
```

## Verification

```bash
# Check all services are running
docker exec <container> ps aux | grep -E 'dwowd|universal_relayer'

# RPC health check
docker exec <container> bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"params\":[],\"id\":1}\" >&3; timeout 3 cat <&3'

# Relayer status
docker exec <container> /app/universal_relayer --config /root/.config/dwow/universal_relayer.toml status

# Chain sync status
docker exec <container> bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.info\",\"params\":[],\"id\":1}" >&3; timeout 3 cat <&3'
```

## Data Persistence

| Path | Contents |
|------|----------|
| `/root/.local/share/dwow/dwowd/` | Blockchain database, mining keys |
| `/root/.local/share/dwow/drk/` | Wallet database (contract registrations) |
| `/root/.config/dwow/dwowd_config.toml` | dwowd configuration |
| `/root/.config/dwow/universal_relayer.toml` | Relayer configuration |

Mount these paths as Docker volumes for data persistence across container
restarts.

## Troubleshooting

**No peers after startup.** Seeds may be unreachable. Check `SEED_ADDR` and
verify network connectivity. If running behind NAT, set `EXTERNAL_ADDR` to
your public IP.

**Contract deployment fails.** Ensure the chain is synced (height > 0) and
the wallet has sufficient funds for deployment fees (42,000,000 DARK per
transaction). Check `docker logs` for specific error messages.

**Relayer can't connect to external chains.** Verify each chain's RPC endpoint
is reachable from the container. Host networking mode (`--network=host`) is
required for the relayer to reach local chain nodes.

**Bridge deposits not detected.** The universal_relayer polls dwowd for pending
withdrawals via JSON-RPC. Check `POLL_INTERVAL_SECS` and ensure the bridge
contract is deployed and initialized.

## Security

All contracts are **EXPERIMENTAL** and **UNAUDITED**. The bridge handles
cross-chain asset transfers — bugs can result in permanent loss of funds.

- Use `WALLET_SECRET_FILE` mounted as a Docker secret, never environment variables
- Store relayer private keys in Docker secrets, not in environment variables
- Run each chain's full node yourself rather than trusting third-party RPCs
- The bridge ZK circuits for chain-specific deposit verification (XMR, ZEC, AZT, LTC)
  contain verification functions that are still under active development
