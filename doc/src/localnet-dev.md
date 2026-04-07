# Local Devnet Setup

## Overview

A local development network (devnet) for DarkFi testing, funded via block mining rather than a broken faucet. Uses RandomX PoW mining against the local darkfid node's stratum server to generate DARK tokens for testing.

## Quick Start

```bash
# Terminal 1: Start darkfid with localnet config
./target/release/darkfid -c contrib/localnet/darkfid-single-node/darkfid.toml

# Terminal 2: Mine blocks to your wallet
./target/release/drk -c bin/drk/drk_config.toml -n localnet mine

# Terminal 3: Check balance
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet balance
```

## How It Works

1. **darkfid** runs a stratum mining server on port 48347 (configured in localnet toml)
2. **drk mine** connects via TCP, logs in with your wallet address as recipient
3. darkfid sends mining jobs (RandomX blob + target)
4. drk mines RandomX hashes in a background thread
5. Shares found are submitted back to stratum server
6. Accepted shares = mined blocks = PoW rewards (20 DARK per block)
7. Wallet scanning discovers the coins

## Key Components

### darkfid (daemon)
- Stratum server: `127.0.0.1:48347`
- RPC endpoint: `127.0.0.1:48345`
- Config: `contrib/localnet/darkfid-single-node/darkfid.toml`
- `pow_fixed_difficulty=1` makes mining fast for testing

### drk (CLI wallet)

**Global flags:**
```
-c, --config <config>      Configuration file to use
-n, --network <network>    Blockchain network to use [default: testnet]
-f, --fun                  Flag for fun
-v                         Increase verbosity (-vvv supported)
```

**Wallet subcommands:**
```
drk wallet address            Get the default address
drk wallet addresses          Print all addresses
drk wallet balance            Query known balances
drk wallet coins              Print all coins
drk wallet default-address    Set default address
drk wallet import-secrets     Import secret keys from stdin
drk wallet initialize         Initialize wallet database
drk wallet keygen             Generate new keypair
drk wallet mining-config      Print wallet address mining configuration
drk wallet secrets            Print all secret keys
drk wallet tree               Print Merkle tree
```

**Contract subcommands:**
```
drk contract deploy <auth> <wasm-path> [deploy-ix]    Deploy a smart contract
drk contract export-data <tx-hash>                     Export wasm bincode + deploy ix
drk contract generate-deploy                          Generate new deploy authority
drk contract list [contract-id]                        List deploy authorities
drk contract lock <deploy-auth>                        Lock a smart contract
```

## Mining Details

- **Algorithm:** RandomX (rx/0)
- **Difficulty:** 1 (very low, shares found frequently)
- **Block reward:** 20 DARK per mined block
- **Blob structure:** 43 bytes = [2 byte padding][40 byte header with nonce at offset 39]
- **Target:** 8 bytes MSB of 32-byte target, padded with zeros to 32 bytes for comparison
- **Nonce:** 4 bytes at blob byte offset 39 (little-endian u32)

## Full Workflow

### 1. Initialize wallet (first time only)
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet initialize
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet keygen
```

### 2. Start localnet
```bash
./target/release/darkfid -c contrib/localnet/darkfid-single-node/darkfid.toml
```

### 3. Mine blocks
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet mine
# Press Ctrl+C when sufficient DARK accumulated
```

### 4. Check balance
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet balance
```

### 5. Scan blockchain
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet scan
# Or reset and rescan from block 0:
./target/release/drk -c bin/drk/drk_config.toml -n localnet scan --reset 0
```

### 6. List known coins
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet coins
```

### 7. Deploy a contract
```bash
# Generate deploy authority if needed
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract generate-deploy

# Deploy contract (pipe output to broadcast)
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract deploy <contract-id> <wasm-path> | ./target/release/drk -c bin/drk/drk_config.toml -n localnet broadcast
```

### 8. Verify deployment
```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract list
```

## Network Ports

| Service | Port | Purpose |
|---------|------|---------|
| darkfid RPC | 48345 | JSON-RPC for wallet commands |
| darkfid stratum | 48347 | Stratum server for block mining |

## Troubleshooting

```bash
# If "Resource temporarily unavailable" error on wallet db:
# Kill any running drk processes
pkill -f "drk.*mine"

# Then retry wallet commands
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet balance
```

## CLI Quirks

### scan is a top-level subcommand, not wallet scan

The `scan` command is not under `wallet` - it's a top-level subcommand:
```bash
drk scan                    # Correct - scan blockchain
drk wallet scan             # Wrong - this doesn't exist
```

This differs from other wallet operations which are under `drk wallet <subcommand>`.

### Config file must be passed explicitly

There is no default config file location. Every command requires `-c`:
```bash
drk -c bin/drk/drk_config.toml -n localnet wallet balance  # Correct
drk -n localnet wallet balance                              # Wrong - fails
```

### --reset uses space, not equals

The `--reset` flag for scan uses space-separated syntax:
```bash
drk scan --reset 0     # Correct - space
drk scan --reset=0    # Wrong - equals sign doesn't work
```

### broadcast reads base64 from stdin

The `broadcast` command reads a base64-encoded transaction from stdin:
```bash
drk contract deploy <auth> <wasm> | drk broadcast  # Pipe output to broadcast
```

### balance shows unspent only

`drk wallet balance` shows only unspent balances. Spent coins are not included in the balance calculation.

### coin values are in raw units

Coin values in `drk wallet coins` output are shown as raw values (e.g., `2000000000`) with a formatted version in parentheses (e.g., `(20)`). The DARK token has 8 decimal places.

### contract list without args lists all authorities

```bash
drk contract list              # Lists ALL deploy authorities
drk contract list <contract>  # Shows history for specific contract
```

The history lookup requires the deployment transaction hash (tx-hash), not the contract ID.

## Contract Deployment Testing (2026-04-07)

Tested WASM contract deployment on localnet.

### Successfully Deployed Contracts

| Contract | WASM Size | Status |
|----------|------------|--------|
| darktoshi_dice | 196KB | ✅ Deployed |
| baccarat | 199KB | ✅ Deployed |
| dao | 320KB | ✅ Deployed |
| dao_escrow | 66KB | ✅ Deployed |
| money | 496KB | ✅ Deployed |
| money_v2 | 496KB | ✅ Deployed |
| escrow | 177KB | ✅ Deployed |
| lottery | 228KB | ✅ Deployed |

### Contracts That Failed to Deploy

| Contract | WASM Size | Error | Likely Cause |
|----------|------------|-------|--------------|
| betting_stake | 380B | Gas estimation failed | Stub/placeholder |
| drain_protection | 383B | Gas estimation failed | Stub/placeholder |
| roulette | 375B | Gas estimation failed | Stub/placeholder |
| bridge | 227KB | ParseFailed | Requires deploy instruction or has bug |
| darkbet_exchange | 313KB | ParseFailed | Requires deploy instruction or has bug |
| dex | 208KB | ParseFailed | Requires deploy instruction or has bug |
| pool_stake | 212KB | ParseFailed | Requires deploy instruction or has bug |
| stablecoin | 85KB | ParseFailed | Requires deploy instruction or has bug |
| relayer_endowment | 181KB | ParseFailed | Requires deploy instruction or has bug |

### Deployment Command

```bash
# Generate authority
drk -c bin/drk/drk_config.toml -n localnet contract generate-deploy

# Deploy (pipe to broadcast)
drk -c bin/drk/drk_config.toml -n localnet contract deploy <auth> <wasm> | \
  drk -c bin/drk/drk_config.toml -n localnet broadcast
```

### Note on Small WASM Files

Contracts with WASM files under 1KB (betting_stake, drain_protection, roulette) are likely stubs or placeholder implementations that don't have actual contract logic.

## File References

- `bin/darkfid/src/rpc/miner.rs` - darkfid stratum server implementation
- `bin/darkfid/src/lib.rs` - `DarkfiNode::is_localnet()` guard
- `bin/drk/src/main.rs` - Subcommand definitions and handlers
- `bin/drk/src/rpc.rs` - `miner_mine()` stratum client
- `bin/drk/drk_config.toml` - Network configuration
- `contrib/localnet/darkfid-single-node/darkfid.toml` - Localnet config