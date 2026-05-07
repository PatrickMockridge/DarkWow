# DarkWow Testnet

DarkWow is a fork of DarkFi. Magic bytes `[68, 82, 75, 87]` encode "DRKW" in ASCII, uniquely identifying DarkWow nodes on the P2P network.

This directory provides a 3-container Docker testnet that mirrors public testnet conditions: `localnet=false`, `skip_fees=false`, `threshold=3`, `target_block_time=120s`, and RandomX mining via xmrig.

## Architecture

Three containers on a bridge network (`darkwow-net`):

| Container | Role | P2P Port | RPC Port | Stratum Port |
|-----------|------|----------|----------|--------------|
| `dwow-lilith` | Seed node | 31340 | — | — |
| `dwow-node0` | Mining node | 31342 | 31345 | 31347 |
| `dwow-node1` | Mining node | 31343 | 31346 | 31348 |

Each mining node runs dwowd + xmrig. Node0 and node1 connect to lilith as their seed, plus each other as peers. xmrig mines via local stratum, and coinbase rewards are paid to an auto-generated mining address.

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
| `pow_target` | 120 |

## Quick Start

```bash
# Build and start
docker compose up --build -d

# Check status
docker compose ps

# View logs
docker compose logs -f

# Tear down
docker compose down
```

## Full Pipeline (build, start, health-check)

```bash
./test_pipeline.sh
```

## Wallet Setup

```bash
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

Tests the full economic cycle: mining → fund → deploy → transfer → fee.

```bash
# Single-contract test (deploy + transfer)
./contract_test.sh

# Multi-contract test (deploy money_v3, DEX, dao_escrow + transfer + attach-fee)
./test-contracts.sh
```

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
| Matches public testnet | No | Yes |

## File Overview

| File | Purpose |
|------|----------|
| `Dockerfile` | Builds dwowd + dww + WASM contracts (deployooor, native_token, money_v3, baccarat) |
| `docker-compose.yml` | 3-container orchestration (lilith, node0, node1) with bridge network |
| `entrypoint.sh` | Config generation + dwowd launch + xmrig launch per container |
| `test_pipeline.sh` | Clean → validate → build → start → health-check |
| `test-contracts.sh` | Multi-contract deploy and transaction test (pass/fail counters) |
| `contract_test.sh` | Single-contract deploy + transfer test |
