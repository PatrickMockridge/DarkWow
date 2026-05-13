# DarkWow Linear-Testnet Docker Setup

A 3-node Docker-based linear-testnet with xmrig for external RandomX mining.
(docker-compose defines lilith + node0 + node1; node2-4 from the architecture
diagram are added by scaling node1.)

## Overview

This setup runs dwowd nodes in `--network linear-testnet` mode, each with an
xmrig miner connected via stratum protocol. Lilith acts as the P2P seed peer;
node0 and node1 connect to lilith and peer with each other.

## Architecture

```
                    +-------------------+
                    |  dwow-local       |
                    |     network       |
                    +-------------------+
                            |
        +-------------------+-------------------+
        |                                       |
    +---+---+                               +---+---+
    |node0  |                               |node1  |
    |P2P:28340|  <----------------------->  |P2P:28341|
    |RPC:28345|                             |RPC:28346|
    |STR:48347|                             |STR:48447|
    +---+---+                               +---+---+
        |                                       |
    +---+---+                               +---+---+
    |xmrig0 |                               |xmrig1 |
    +---+---+                               +---+---+
```

## Ports

| Node | RPC | Stratum | P2P |
|------|-----|---------|-----|
| lilith | — | — | 18345 |
| node0 | 28345 | 48347 | 28340 |
| node1 | 28346 | 48447 | 28341 |

## Prerequisites

- Docker installed (`docker compose` plugin)
- dwowd binary built (or build from source in Docker)

## Quick Start

### 1. Build dwowd binary (if not using pre-built)

```bash
cargo build -p dwowd --release
```

### 2. Copy binary to docker context

The docker-compose expects the binary at `../../target/release/dwowd`. If your
build is elsewhere, adjust the Dockerfile.

### 3. Start the stack

```bash
cd contrib/docker/linear-testnet
docker compose up -d
```

### 4. Check status

```bash
docker compose ps
```

### 5. View logs

```bash
docker compose logs -f           # all logs
docker compose logs node0        # node0 only
docker compose logs xmrig0       # xmrig0 connects via entrypoint
```

### 6. Check block height

```bash
# dwowd uses raw TCP JSON-RPC (not HTTP). Use dww or netcat:
echo '{"jsonrpc":"2.0","method":"blockchain.get_block_linear","params":[1],"id":1}' | nc -w1 localhost 28345
```

### 7. Mine via RPC

```bash
# Use the miner.mine_linear RPC with a wallet address and reward amount
curl -X POST http://localhost:28345 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "miner.mine_linear",
    "params": ["<wallet_address>", 100000000],
    "id": 1
  }'
```

### 8. Stop the stack

```bash
docker compose down
```

## Wallet Setup

Each node has an associated wallet for receiving mining rewards and deploying
contracts.

### Generate Wallets

```bash
# Generate wallet configurations
cd wallets
./setup_wallets.sh

# Generate addresses (requires running stack)
./generate_addresses.sh
```

This creates:
- `wallets/wallet0/drk0.toml` - Wallet 0 config (connects to node0:28345)
- `wallets/wallet1/drk1.toml` - Wallet 1 config (connects to node1:28346)
- ... etc

### Initialize Wallets

```bash
# Initialize each wallet (creates keys)
for i in 0 1 2 3 4; do
    ../../target/debug/dww -c wallets/wallet$i/drk$i.toml wallet init
done

# Get addresses
for i in 0 1 2 3 4; do
    echo "Wallet $i: $(../../target/debug/dww -c wallets/wallet$i/drk$i.toml wallet address)"
done
```

### Use with Docker

Set `WALLET_ADDRESS` and `WALLET_SECRET` environment variables:

```bash
# Start with specific wallet
WALLET_ADDRESS=<wallet_addr> \
WALLET_SECRET=<wallet_secret> \
docker compose up -d
```

## Contract Deployment

Use `dww` to deploy and test smart contracts via the Deployooor genesis
contract:

```bash
# Deploy a contract using wallet 0
./target/debug/dww -c wallets/wallet0/drk0.toml contract deploy <wasm_file>

# Invoke a contract
./target/debug/dww -c wallets/wallet0/drk0.toml contract invoke <contract_id> <function> <args>
```

## Verification

### Check node health (raw TCP JSON-RPC)

```bash
echo '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}' | nc -w1 localhost 28345
```

### Check block height

```bash
echo '{"jsonrpc":"2.0","method":"blockchain.get_block_linear","params":[1],"id":1}' | nc -w1 localhost 28345
```

### Check xmrig is connected

```bash
docker compose logs node0 2>&1 | grep -i "stratum\|xmrig\|job\|submit"
```

## Troubleshooting

### Nodes not connecting to each other

Check that `skip_sync = true` is set and all nodes use lilith as seed peer.

### xmrig not connecting to stratum

Check that stratum RPC is enabled and port mapping is correct. xmrig connects to
`node0:48347` (container internal) which maps to host `127.0.0.1:48347`.

### Build fails

The Dockerfile supports two modes:
1. **Default**: Copy pre-built binary (faster, needs host build)
2. **BUILD_FROM_SOURCE=1**: Build in Docker (slower, self-contained)

To build from source:
```bash
docker build . -t dwow:linear-testnet --build-arg BUILD_FROM_SOURCE=1
```

## Files

```
contrib/docker/linear-testnet/
├── docker-compose.yml    # Stack orchestration
├── Dockerfile            # dwowd + xmrig image
├── entrypoint.sh         # Config generation + startup
├── node0.toml ... node4.toml  # Per-node configs
├── wallets/               # Wallet configurations
│   ├── setup_wallets.sh      # Create wallet configs
│   ├── generate_addresses.sh # Generate wallet addresses
│   └── test_transactions.sh # Test transaction script
├── test_pipeline.sh       # CI pipeline
└── README.md              # This file
```

## Genesis Contracts

At startup, dwowd nodes deploy genesis contracts at block 1:
- **Deployooor**: Enables WASM contract deployment via `DeployV1` calls
- **NativeToken**: Consensus token for network fees (FeeV1) and block rewards
  (PoWRewardV1)

These are deployed by `build_genesis_config()` in `bin/darkfid/src/genesis.rs`.

## See Also

- [Linear Blockchain Architecture](../../doc/src/arch/linear_blockchain.md)
- [Uncle Merkle Consensus](../../doc/src/arch/consensus/uncle_merkle.md)
- [DarkWow Testnet README](../darkwow-testnet/README.md)
