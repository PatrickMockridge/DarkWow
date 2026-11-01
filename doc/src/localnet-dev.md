# Local Devnet Setup

> [!NOTE]
> This document includes both permanent reference material and dated operational
> logs from April 2026. For the current testing taxonomy, see:
> - [Level 1: Lightweight Tests](./dev/testing/level-1-lightweight.md)
> - [Level 2: Heavyweight Tests](./dev/testing/level-2-heavyweight.md)
> - [Level 3: Containerized Localnet](./dev/testing/level-3-localnet.md)
> - [Level 4: Containerized Devnet Node](./dev/testing/level-4-devnet.md)

## Overview

A local development network (devnet) for DarkWow testing, funded via block mining rather than a broken faucet. Uses RandomX PoW mining against the local dwowd node's stratum server to generate DRKW tokens for testing.

## Quick Start

```bash
# Terminal 1: Start a single genesis-mining node. `darkwow node` execs dwowd with
# MINING_ENABLED=true and CREATE_GENESIS=true for the genesis role.
./target/release/darkwow node --role genesis --identity node0

# Terminal 2: Check balance (the node's built-in miner mines; there is no wallet
# `mine` command).
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n darkwow-devnet wallet balance
```

## How It Works

1. **dwowd** runs the chain and, with `MINING_ENABLED=true` (the `genesis`/`miner` roles),
   its built-in `miner_task` mines once sync reaches `CaughtUp`.
2. Each mined block pays the node's declared key: PoW rewards (~13.8 DRKW at height 1,
   decaying — see the emission curve's `expected_reward`).
3. The wallet scans the chain and discovers the commitments. External xmrig miners can
   instead submit through the stratum server (port 48347) if configured; that is an
   alternative submission path, not a wallet command.

## Key Components

### dwowd (daemon)
- Stratum server: `127.0.0.1:48347`
- RPC endpoint: `127.0.0.1:48345`
- Config: `bin/dwowd/dwowd_config.toml`
- `pow_fixed_difficulty=1` makes mining fast for testing

### dwow_wallet (CLI wallet)

**Global flags:**
```
-c, --config <config>      Configuration file to use
-n, --network <network>    Blockchain network to use [default: testnet]
-f, --fun                  Flag for fun
-v                         Increase verbosity (-vvv supported)
```

**Top-level commands** (`bin/dww/src/args.rs`, the `WalletCommand` enum):

```
dwow_wallet transfer <amount> <asset-id> <recipient>   Send a note
dwow_wallet redeem <cap-id>                            Redeem a capability
dwow_wallet burn <cap-id>...                           Burn capabilities
dwow_wallet broadcast                                  Broadcast a raw transaction
dwow_wallet scan [--reset <height>]                    Rescan the chain
dwow_wallet sync <init|status>                         Sync bookkeeping
dwow_wallet daemon                                     Run the wallet daemon
dwow_wallet position                                   List positions from the capability catalogues
dwow_wallet diagnostic                                 Print diagnostic information
dwow_wallet wallet <subcommand>                        Wallet database operations (below)
dwow_wallet contract <subcommand>                      Contract operations (below)
```

**Wallet subcommands** (`WalletSubcmd`) — the enum has exactly these eight:

```
dwow_wallet wallet initialize               Initialize the wallet database
dwow_wallet wallet address                  Get the default address
dwow_wallet wallet addresses                Print all addresses
dwow_wallet wallet default-address <index>  Set the default address
dwow_wallet wallet balance [--porcelain]    Query known balances
dwow_wallet wallet secrets                  Print all secret keys
dwow_wallet wallet tree                     Print the Merkle tree
dwow_wallet wallet capabilities             List the wallet's capabilities
```

**Contract subcommands** (`ContractSubcmd`):

```
dwow_wallet contract deploy <deploy-auth> <wasm-path> [deploy-ix] [manifest]  Deploy a contract
dwow_wallet contract show <contract-id>                                       Print a contract
dwow_wallet contract lock <deploy-auth>                                       Lock a contract
dwow_wallet contract invoke <contract-id> <function> [params] [recipient]     Call a function
```

**This reference listed subcommands the parser rejects** (`F-601`/`F-602`). The wallet block named
four that are not variants of `WalletSubcmd` — `commitments`, `import-secrets`, `keygen` and
`mining-config` — and the contract block named six more that are not variants of `ContractSubcmd`
(`dao-escrow-init`, `drain-protection-init`, `enable-drain-protection`, `export-data`,
`generate-deploy`, `list`). Ten rows, none of which resolves. The lists above are the enums' own
variants; run `dwow_wallet wallet --help` rather than trusting a copied list.

**Universal Contract Invocation:**
```
dwow_wallet contract invoke <contract-name-or-id> <function> [--params <json-file>]

# Example: pay a DAO-Escrow premium
dwow_wallet contract invoke dao_escrow pay_premium --params params.json

# params.json must match the manifest's schema for the selected function.
# `enable_drain_protection` (0x06) was retired and is not a valid selector.
```

## Mining Details

- **Algorithm:** RandomX (rx/0)
- **Difficulty:** 1 (very low, shares found frequently)
- **Block reward:** ~**13.8 DRKW** per mined block at height 1 — `INITIAL_REWARD` is
  `1_383_764_049` base units and a DRKW is `10^8`, so the emission curve's first block is 13.83764049
  DRKW, and it decays from there. This line said "20 DRKW" until 2026-10-12 (`F-601`); the figure for
  any given height is `expected_reward(h)` in `src/sdk/src/blockchain.rs`.
- **Blob structure:** 292 bytes (`BlockHeader::MINING_BLOB_LEN`) — see
  [stratum.md §292-Byte Mining Blob Layout](arch/consensus/stratum.md#292-byte-mining-blob-layout)
- **Target:** 8 bytes MSB of 32-byte target, padded with zeros to 32 bytes for comparison
- **Nonce:** 4 bytes at blob byte offset 39 (little-endian u32)

## Full Workflow

### 1. Initialize wallet (first time only)

See [Wallet Architecture](arch/wallet.md) for wallet initialization and keygen.
Use `-c bin/dww/dww_config.toml -n darkwow-devnet` for localnet dev.

### 2. Start localnet
`contrib/localnet/` does not exist. Start a single genesis-mining node through the
top-level CLI (it execs `dwowd` with `MINING_ENABLED=true` and `CREATE_GENESIS=true`):

```bash
./target/release/darkwow node --role genesis --identity node0
```

### 3. Mine blocks
There is no wallet `mine` command. The node's built-in `miner_task` mines once sync
reaches `CaughtUp`; for a one-off block, call the node's `miner.mine_linear` RPC. The
wallet must use the **same** network as the daemon (`-n darkwow-devnet`, not `localnet`).

### 4. Check balance
```bash
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n darkwow-devnet wallet balance
```

### 5. Scan blockchain
```bash
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n darkwow-devnet scan
# Or reset and rescan from block 0:
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n darkwow-devnet scan --reset 0
```

### 6. List known capabilities
```bash
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n darkwow-devnet wallet capabilities
```

### 7. Deploy a contract
```bash
# Generate deploy authority if needed
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n darkwow-devnet contract generate-deploy

# Deploy contract (pipe output to broadcast)
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n darkwow-devnet contract deploy <contract-id> <wasm-path> | ./target/release/dwow_wallet -c bin/dww/dww_config.toml -n darkwow-devnet broadcast
```

### 8. Verify deployment
```bash
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n darkwow-devnet contract list
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
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n darkwow-devnet wallet balance
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
dwow_wallet -c bin/dww/dww_config.toml -n darkwow-devnet wallet balance  # Correct
dwow_wallet -n darkwow-devnet wallet balance                              # Wrong - fails
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

`dwow_wallet wallet balance` shows only unspent balances. Spent commitments are not included in the balance calculation.

### commitment values are in raw units

Commitment values in `dwow_wallet wallet commitments` output are shown as raw values (e.g., `2000000000`) with a formatted version in parentheses (e.g., `(20)`). The DRKW token has 8 decimal places.

### contract list without args lists all authorities

```bash
dwow_wallet contract list              # Lists ALL deploy authorities
dwow_wallet contract list <contract>  # Shows history for specific contract
```

The history lookup requires the deployment transaction hash (tx-hash), not the contract ID.
