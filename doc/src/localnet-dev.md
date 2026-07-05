# Local Devnet Setup

> [!NOTE]
> This document includes both permanent reference material and dated operational
> logs from April 2026. For the current testing taxonomy, see:
> - [Level 1: Lightweight Tests(./dev/testing/level-1-lightweight.md)
> - [Level 2: Heavyweight Tests(./dev/testing/level-2-heavyweight.md)
> - [Level 3: Containerized Localnet(./dev/testing/level-3-localnet.md)
> - [Level 4: Containerized Devnet Node(./dev/testing/level-4-devnet.md)

## Overview

A local development network (devnet) for DarkWow testing, funded via block mining rather than a broken faucet. Uses RandomX PoW mining against the local dwowd node's stratum server to generate DRKW tokens for testing.

## Quick Start

```bash
# Terminal 1: Start dwowd with localnet config
./target/release/dwowd -c contrib/localnet/dwowd-single-node/dwowd.toml

# Terminal 2: Mine blocks to your wallet
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet mine

# Terminal 3: Check balance
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet wallet balance
```

## How It Works

1. **dwowd** runs a stratum mining server on port 48347 (configured in localnet toml)
2. **dwow_wallet mine** connects via TCP, logs in with your wallet address as recipient
3. dwowd sends mining jobs (RandomX blob + target)
4. dwow_wallet mines RandomX hashes in a background thread
5. Shares found are submitted back to stratum server
6. Accepted shares = mined blocks = PoW rewards (20 DRKW per block)
7. Wallet scanning discovers the coins

## Key Components

### dwowd (daemon)
- Stratum server: `127.0.0.1:48347`
- RPC endpoint: `127.0.0.1:48345`
- Config: `contrib/localnet/dwowd-single-node/dwowd.toml`
- `pow_fixed_difficulty=1` makes mining fast for testing

### dwow_wallet (CLI wallet)

**Global flags:**
```
-c, --config <config>      Configuration file to use
-n, --network <network>    Blockchain network to use [default: testnet]
-f, --fun                  Flag for fun
-v                         Increase verbosity (-vvv supported)
```

**Wallet subcommands:**
```
dwow_wallet wallet address            Get the default address
dwow_wallet wallet addresses          Print all addresses
dwow_wallet wallet balance            Query known balances
dwow_wallet wallet coins              Print all coins
dwow_wallet wallet default-address    Set default address
dwow_wallet wallet import-secrets     Import secret keys from stdin
dwow_wallet wallet initialize         Initialize wallet database
dwow_wallet wallet keygen             Generate new keypair
dwow_wallet wallet mining-config      Print wallet address mining configuration
dwow_wallet wallet secrets            Print all secret keys
dwow_wallet wallet tree               Print Merkle tree
```

**Contract subcommands:**
```
dwow_wallet contract dao-escrow-init <dao-bulla> <token-id>    Initialize a DAO-Escrow endowment
dwow_wallet contract drain-protection-init <fund-id> <spend-auth> <dao-bulla>  Initialize DrainProtection
dwow_wallet contract enable-drain-protection <dao-bulla> <drain-bulla>          Enable drain protection
dwow_wallet contract deploy <auth> <wasm-path> [deploy-ix]    Deploy a smart contract
dwow_wallet contract export-data <tx-hash>                     Export wasm bincode + deploy ix
dwow_wallet contract generate-deploy                          Generate new deploy authority
dwow_wallet contract invoke <contract-id> <function>           Invoke a contract function
dwow_wallet contract list [contract-id]                        List deploy authorities
dwow_wallet contract lock <deploy-auth>                        Lock a smart contract
```

**Universal Contract Invocation:**
```
dwow_wallet contract invoke <contract-name-or-id> <function> [--params <json-file>]

# Example: Enable drain protection on DAO-Escrow
dwow_wallet contract invoke dao_escrow enable_drain_protection --params params.json

# Where params.json contains:
# {"dao_escrow_bulla": "...", "drain_protection_bulla": "..."}
```

## Mining Details

- **Algorithm:** RandomX (rx/0)
- **Difficulty:** 1 (very low, shares found frequently)
- **Block reward:** 20 DRKW per mined block
- **Blob structure:** 43 bytes = [2 byte padding][40 byte header with nonce at offset 39]
- **Target:** 8 bytes MSB of 32-byte target, padded with zeros to 32 bytes for comparison
- **Nonce:** 4 bytes at blob byte offset 39 (little-endian u32)

## Full Workflow

### 1. Initialize wallet (first time only)

See [Wallet Architecture](arch/wallet.md) for wallet initialization and keygen.
Use `-c bin/dww/dww_config.toml -n localnet` for localnet dev.

### 2. Start localnet
```bash
./target/release/dwowd -c contrib/localnet/dwowd-single-node/dwowd.toml
```

### 3. Mine blocks
```bash
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet mine
# Press Ctrl+C when sufficient DRKW accumulated
```

### 4. Check balance
```bash
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet wallet balance
```

### 5. Scan blockchain
```bash
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet scan
# Or reset and rescan from block 0:
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet scan --reset 0
```

### 6. List known coins
```bash
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet wallet coins
```

### 7. Deploy a contract
```bash
# Generate deploy authority if needed
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet contract generate-deploy

# Deploy contract (pipe output to broadcast)
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet contract deploy <contract-id> <wasm-path> | ./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet broadcast
```

### 8. Verify deployment
```bash
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet contract list
```

## Network Ports

| Service | Port | Purpose |
|---------|------|---------|
| dwowd RPC | 48345 | JSON-RPC for wallet commands |
| dwowd stratum | 48347 | Stratum server for block mining |

## Troubleshooting

```bash
# If "Resource temporarily unavailable" error on wallet db:
# Kill any running dwow_wallet processes
pkill -f "dwow_wallet.*mine"

# Then retry wallet commands
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet wallet balance
```

## CLI Quirks

### scan is a top-level subcommand, not wallet scan

The `scan` command is not under `wallet` - it's a top-level subcommand:
```bash
dwow_wallet scan                    # Correct - scan blockchain
dwow_wallet wallet scan             # Wrong - this doesn't exist
```

This differs from other wallet operations which are under `dwow_wallet wallet <subcommand>`.

### Config file must be passed explicitly

There is no default config file location. Every command requires `-c`:
```bash
dwow_wallet -c bin/dww/dww_config.toml -n localnet wallet balance  # Correct
dwow_wallet -n localnet wallet balance                              # Wrong - fails
```

### --reset uses space, not equals

The `--reset` flag for scan uses space-separated syntax:
```bash
dwow_wallet scan --reset 0     # Correct - space
dwow_wallet scan --reset=0    # Wrong - equals sign doesn't work
```

### broadcast reads base64 from stdin

The `broadcast` command reads a base64-encoded transaction from stdin:
```bash
dwow_wallet contract deploy <auth> <wasm> | dwow_wallet broadcast  # Pipe output to broadcast
```

### balance shows unspent only

`dwow_wallet wallet balance` shows only unspent balances. Spent coins are not included in the balance calculation.

### coin values are in raw units

Coin values in `dwow_wallet wallet coins` output are shown as raw values (e.g., `2000000000`) with a formatted version in parentheses (e.g., `(20)`). The DRKW token has 8 decimal places.

### contract list without args lists all authorities

```bash
dwow_wallet contract list              # Lists ALL deploy authorities
dwow_wallet contract list <contract>  # Shows history for specific contract
```

The history lookup requires the deployment transaction hash (tx-hash), not the contract ID.


## Historical Testing Logs

Dated contract deployment and debugging records from April 2026 have been
moved to [Localnet Contract Testing Logs](changelogs/2026-04-localnet-testing.md).
