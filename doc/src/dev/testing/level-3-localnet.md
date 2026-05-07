# Level 3: Containerized Localnet

A multi-container Docker testnet that mirrors public testnet conditions. Runs
on a single machine with 3 containers: a seed node (lilith) and 2 mining nodes
(dwowd + xmrig).

**Location:** `contrib/docker/darkwow-testnet/`

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│ dwow-lilith  │     │ dwow-node0   │     │ dwow-node1   │
│ (seed)       │◄───►│ (miner)      │◄───►│ (miner)      │
│              │     │              │     │              │
│ P2P: 31340   │     │ P2P: 31342   │     │ P2P: 31343   │
│              │     │ RPC: 31345   │     │ RPC: 31346   │
│              │     │ Stratum:31347│     │ Stratum:31348│
└──────────────┘     └──────────────┘     └──────────────┘
       │                     │                     │
       └─────────────────────┴─────────────────────┘
                  Bridge network: dwow-local
```

| Container | Role | P2P Port | RPC Port | Stratum Port |
|-----------|------|----------|----------|--------------|
| `dwow-lilith` | Seed node | 31340 | — | — |
| `dwow-node0` | Mining node | 31342 | 31345 | 31347 |
| `dwow-node1` | Mining node | 31343 | 31346 | 31348 |

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

# Build and start all 3 containers
docker compose up --build -d

# Check status
docker compose ps

# View logs
docker compose logs -f

# Full pipeline (clean → build → start → health-check)
./test_pipeline.sh

# Tear down
docker compose down
```

## Mining

Both node0 and node1 mine via xmrig connecting to their local stratum servers.
Mining address resolution follows a three-tier priority:

1. `WALLET_ADDRESS` environment variable (operator-provided)
2. Persisted file from a prior dwowd run
3. Auto-generated keypair on first dwowd start

Block reward is 20 DRKW per block. At `pow_fixed_difficulty = 1`, blocks
mine instantly; for realistic mining, set a higher difficulty.

## Wallet Setup

```bash
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

```bash
# Single-contract test (deploy + transfer)
./contract_test.sh

# Multi-contract test (deploy money_v3, DEX, dao_escrow + transfer + fee)
./test-contracts.sh
```

The contract tests exercise the full economic cycle: mining → fund wallet →
deploy WASM contract → transfer tokens → pay fees.

## Linear-Testnet Variant

A 5-node variant is available at `contrib/docker/linear-testnet/` with relaxed parameters:

| Feature | `darkwow-testnet` | `linear-testnet` |
|---------|-------------------|------------------|
| `localnet` | `false` | `true` |
| Magic bytes | `[68, 82, 75, 87]` | `[163, 139, 113, 101]` |
| Threshold | 3 | 1 |
| `pow_target` | 120 | 1 |
| Block time | 120s | ~60s |
| Nodes | 3 (seed + 2 miners) | 5 (seed + 4 miners + xmrig each) |

Use `linear-testnet` when you need more nodes for consensus testing or want
faster block times. Use `darkwow-testnet` when you need parameters matching
the public testnet.

## File Overview

| File | Purpose |
|------|----------|
| `Dockerfile` | Multi-stage build: zkas → WASM contracts → dwowd + lilith |
| `docker-compose.yml` | 3-container orchestration on bridge network |
| `entrypoint.sh` | Per-container config generation + dwowd + xmrig launch |
| `test_pipeline.sh` | Clean → validate → build → start → health-check |
| `test-contracts.sh` | Multi-contract deploy and transaction test |
| `contract_test.sh` | Single-contract deploy + transfer test |
