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
3. Auto-generated keypair on first dwowd start (if no secret provided)

Block reward is 20 DRKW per block. At `pow_fixed_difficulty = 1`, blocks
mine instantly; for realistic mining, set a higher difficulty.

## Wallet Setup

The wallet is initialized by `test_pipeline.sh` with the same mining keypair
used by the nodes. No secret extraction from containers is needed — `dww scan`
decrypts coinbase AEAD-encrypted notes client-side using the wallet's local
secret key.

```bash
NETWORK="darkwow-testnet"
DRK="./target/release/dww"

# Verify wallet has keys (test_pipeline.sh must complete first)
$DRK -n $NETWORK wallet address

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

## Dwow-Devnet Variant

A 3-node bridge-networked variant is available at `contrib/docker/dwow-devnet/`
with relaxed parameters for rapid local iteration:

| Feature | `darkwow-testnet` | `dwow-devnet` |
|---------|-------------------|---------------|
| `localnet` | `false` | `false` |
| Magic bytes | `[68, 82, 75, 87]` | auto-derived |
| Threshold | 3 | 1 |
| `pow_target` | 120 | 120 |
| `fixed_difficulty` | auto-adjusting | 1 (instant blocks) |
| `skip_fees` | `false` | `true` |
| `skip_sync` | `false` | `true` |
| Nodes | 3 (seed + 2 miners) | 3 (lilith + 2 miners) |
| Networking | Bridge (port-mapped) | Bridge (default) or Host |

Use `dwow-devnet` for fast local contract testing. Use `darkwow-testnet` when
you need parameters matching the public testnet.

## File Overview

| File | Purpose |
|------|----------|
| `Dockerfile` | Multi-stage build: zkas → WASM contracts → dwowd + lilith |
| `docker-compose.yml` | 3-container orchestration on bridge network |
| `entrypoint.sh` | Per-container config generation + dwowd + xmrig launch |
| `test_pipeline.sh` | Clean → validate → build → start → health-check |
| `test-contracts.sh` | Multi-contract deploy and transaction test |
| `contract_test.sh` | Single-contract deploy + transfer test |
