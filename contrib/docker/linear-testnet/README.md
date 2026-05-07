# DarkWow Linear-Testnet Docker Setup

A 5-node Docker-based linear-testnet with xmrig for external RandomX mining.

## Overview

This setup runs 5 darkfid nodes in `--network linear-testnet` mode, each with an xmrig miner connected via stratum protocol. Node0 acts as the seed peer for the network.

## Architecture

```
                    +-------------------+
                    |  dwow-local     |
                    |     network       |
                    +-------------------+
                            |
        +-------------------+-------------------+
        |                   |                   |
    +---+---+           +---+---+           +---+---+
    |node0  |           |node1  |           |node2  |
    |P2P:28340|  <----  |P2P:28341|          |P2P:28342|
    |RPC:28345|         |RPC:28346|          |RPC:28347|
    |STR:48347|         |STR:48447|          |STR:48547|
    +---+---+           +---+---+           +---+---+
        |                   |                   |
    +---+---+           +---+---+           +---+---+
    |xmrig0 |           |xmrig1 |           |xmrig2 |
    +---+---+           +---+---+           +---+---+

    (node3/node4 follow same pattern)
```

## Ports

| Node | RPC | Stratum | P2P |
|------|-----|---------|-----|
| node0 | 28345 | 48347 | 28340 |
| node1 | 28346 | 48447 | 28341 |
| node2 | 28347 | 48547 | 28342 |
| node3 | 28348 | 48647 | 28343 |
| node4 | 28349 | 48747 | 28344 |

## Prerequisites

- Docker and docker-compose installed
- darkfid binary built (or build from source in Docker)

## Quick Start

### 1. Build darkfid binary (if not using pre-built)

```bash
cargo build -p darkfid --release
```

### 2. Copy binary to docker context

The docker-compose expects the binary at `../../target/release/darkfid`. If your build is elsewhere, you may need to adjust the Dockerfile.

### 3. Start the stack

```bash
cd contrib/docker/linear-testnet
./scripts/start.sh
```

### 4. Check status

```bash
docker-compose ps
```

### 5. View logs

```bash
./scripts/logs.sh all        # all logs
./scripts/logs.sh node0       # node0 only
./scripts/logs.sh xmrig0     # xmrig0 only
```

### 6. Mine a block via RPC

```bash
./scripts/mine.sh 0 100000000   # mine on node0 with 1 token reward
```

Or use the RPC directly:

```bash
curl -X POST http://localhost:28345 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "miner.mine_linear",
    "params": ["DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf", 100000000],
    "id": 1
  }'
```

### 7. Stop the stack

```bash
./scripts/stop.sh
```

## Wallet Setup

Each node has an associated wallet for receiving mining rewards and deploying contracts.

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

Update `WALLET_ADDR_0` through `WALLET_ADDR_4` environment variables:

```bash
# Start with specific wallets
WALLET_ADDR_0=<wallet0_addr> \
WALLET_ADDR_1=<wallet1_addr> \
docker-compose up -d
```

## Minting Tokens

Each wallet mints native tokens by mining blocks:

```bash
# Mine to wallet 0
./scripts/mine.sh 0 100000000  # 1 token with 8 decimals

# Mine to wallet 1
curl -X POST http://localhost:28346 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "miner.mine_linear",
    "params": ["<wallet1_address>", 100000000],
    "id": 1
  }'
```

## Contract Deployment

Use `dww` to deploy and test smart contracts:

```bash
# Deploy a contract using wallet 0
./target/debug/dww -c wallets/wallet0/drk0.toml contract deploy <wasm_file>

# Invoke a contract
./target/debug/dww -c wallets/wallet0/drk0.toml contract invoke <contract_id> <function> <args>
```

## Verification

### Check node health

```bash
curl -X POST http://localhost:28345 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}'
```

### Check block height

```bash
curl -X POST http://localhost:28345 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"blockchain.best_fork_next_block_height","params":[],"id":1}'
```

### Check xmrig is connected

```bash
docker-compose logs xmrig0 2>&1 | grep -i "connected\|job\|submit"
```

## Troubleshooting

### Nodes not connecting to each other

Check that `skip_sync = true` is set and all nodes are using `tcp+tls://node0:28340` as peer.

### xmrig not connecting to stratum

Check that stratum RPC is enabled and port mapping is correct. xmrig connects to `node0:48347` (container internal) which maps to host `127.0.0.1:48347`.

### Build fails

The Dockerfile supports two modes:
1. **Default**: Copy pre-built binary (faster, needs host build)
2. **BUILD_FROM_SOURCE=1**: Build in Docker (slower, self-contained)

To build from source:
```bash
docker build . -t darkfi:linear-testnet --build-arg BUILD_FROM_SOURCE=1
```

## Files

```
contrib/docker/linear-testnet/
├── docker-compose.yml    # Stack orchestration
├── Dockerfile            # darkfid image
├── node0.toml ... node4.toml  # Per-node configs
├── wallets/               # Wallet configurations
│   ├── setup_wallets.sh      # Create wallet configs
│   ├── generate_addresses.sh # Generate wallet addresses
│   └── test_transactions.sh # Test transaction script
├── scripts/
│   ├── start.sh      # Start the stack
│   ├── stop.sh       # Stop the stack
│   ├── logs.sh       # View logs
│   └── mine.sh       # Mine via RPC
└── README.md          # This file
```

## Contract Deployment

At startup, darkfid nodes automatically deploy:
- **Deployooor contract**: Enables further contract deployment
- **NativeToken contract**: Consensus token for fees/rewards

These are deployed via `Darkfid::init_linear()` in `bin/darkfid/src/lib.rs`.

## See Also

- [Linear Blockchain Architecture](../../arch/linear_blockchain.md)
- [Uncle Merkle Consensus](../../arch/uncle_merkle.md)
- [DarkWow Testnet Mining](../testnet/merge-mining.md)