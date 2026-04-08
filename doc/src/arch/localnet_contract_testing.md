# Local DarkFi Smart Contract Testing

## Overview

This guide covers localnet smart contract testing in DarkFi using the `drk` CLI wallet with block mining to fund the wallet.

## Current State (2026-04-08)

Localnet smart contract testing now **works fully** with the following workflow:

1. Start `darkfid` with localnet configuration
2. Mine blocks using `drk mine` (RandomX PoW)
3. Scan blockchain to discover coins
4. Deploy contracts using `drk contract deploy`

## Verified Contracts

The following contracts have been verified to work on localnet and/or have passing integration tests:

### Identity Contract (darkfi-identity-contract)
- **Status**: Fully verified on localnet
- **Contract ID**: `9AhecnZbDH4npo3zg8VdYQpSb9jj6nqC3dR7HhuvEWAQ`
- **Integration Tests**: 15 tests passing
- **Test Command**: `cargo test -p darkfi_identity_contract --test integration`
- **Deployment**: Successfully deployed to localnet
- **Tests Cover**: Function enum parsing, data structure encoding/decoding, model invariants (Attribute, Claim, Credential, Issuer types)

### Betting Contracts

| Contract | Build | Integration Tests | Localnet Verified |
|----------|-------|-----------------|------------------|
| baccarat | ✓ | 20 tests | Not yet |
| lottery | ✓ | 6 tests | Not yet |
| roulette | ✓ | None | Not yet |
| slot | ✓ | None | Not yet |
| darktoshi_dice | ✓ | None | Not yet |

### DeFi Contracts

| Contract | Build | Integration Tests |
|----------|-------|------------------|
| darkbet_exchange | ✓ | 30 tests |
| pool_stake | ✓ | 23 tests |
| relayer_endowment | ✓ | 20 tests |

All DeFi contracts were verified with `cargo build` and have passing integration tests.

## Prerequisites

- Compiled DarkFi binaries (`darkfid`, `drk`)
- Config files in `contrib/localnet/darkfid-single-node/`

## Quick Start

```bash
# Terminal 1: Start darkfid with localnet config
./target/release/darkfid -c contrib/localnet/darkfid-single-node/darkfid.toml

# Terminal 2: Mine blocks to your wallet
./target/release/drk -c bin/drk/drk_config.toml -n localnet mine

# Terminal 3: Check balance
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet balance
```

## Full Workflow

### 1. Initialize wallet (first time only)

```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet initialize
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet keygen
```

### 2. Start localnet node

```bash
./target/release/darkfid -c contrib/localnet/darkfid-single-node/darkfid.toml
```

The localnet config uses:
- `pow_fixed_difficulty=1` for fast mining
- Stratum server on port `48347`
- RPC endpoint on port `48345`

### 3. Mine blocks

```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet mine
# Press Ctrl+C when sufficient DARK accumulated (20 DARK per block)
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
# Generate deploy authority
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract generate-deploy

# Deploy contract (pipe output to broadcast)
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract deploy <contract-id> <wasm-path> | ./target/release/drk -c bin/drk/drk_config.toml -n localnet broadcast
```

### 8. Verify deployment

```bash
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract list
```

## Running Integration Tests

All contracts with integration tests can be tested using cargo:

```bash
# Run integration tests for a specific contract
cargo test -p darkfi_identity_contract --test integration
cargo test -p darkfi_baccarat_contract --test integration
cargo test -p darkfi_lottery_contract --test integration
cargo test -p darkfi_darkbet_exchange_contract --test integration
cargo test -p darkfi_pool_stake_contract --test integration
cargo test -p darkfi_relayer_endowment_contract --test integration

# Run all contract tests
cargo test --workspace --exclude darkfi --exclude darkfi_money_contract --exclude darkfi_dao_contract
```

## Available drk Commands

### Global Flags

```
-c, --config <config>      Configuration file to use
-n, --network <network>    Blockchain network to use [default: testnet]
-f, --fun                  Flag for fun
-v                         Increase verbosity (-vvv supported)
```

### Wallet Subcommands

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

### Contract Subcommands

```
drk contract deploy <auth> <wasm-path> [deploy-ix]    Deploy a smart contract
drk contract export-data <tx-hash>                     Export wasm bincode + deploy ix
drk contract generate-deploy                          Generate new deploy authority
drk contract list [contract-id]                      List deploy authorities
drk contract lock <deploy-auth>                       Lock a smart contract
```

### Other Useful Commands

```
drk scan [--reset <height>]    Scan the blockchain
drk mine                       Mine blocks (LOCALNET ONLY)
drk broadcast                   Broadcast a transaction from stdin
drk token list                  List available tokens
drk alias                       Manage token aliases
```

## Network Ports

| Service | Port | Purpose |
|---------|------|---------|
| darkfid RPC | 48345 | JSON-RPC for wallet commands |
| darkfid stratum | 48347 | Stratum server for block mining |

## Configuration

The `drk` CLI requires a config file passed via `-c` flag. The config file at `bin/drk/drk_config.toml` contains network configurations:

```toml
network = "testnet"

[network_config."localnet"]
cache_path = "~/.local/share/darkfi/drk/localnet/cache"
wallet_path = "~/.local/share/darkfi/drk/localnet/wallet.db"
wallet_pass = "testpassword123"
endpoint = "tcp://127.0.0.1:48345"
history_path = "~/.local/share/darkfi/drk/localnet/history.txt"
```

## Troubleshooting

### "Resource temporarily unavailable" on wallet db

```bash
# Kill any running drk processes
pkill -f "drk.*mine"

# Then retry wallet commands
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet balance
```

### Mining not working

Ensure `darkfid` is running with the localnet config and the stratum server is active on port 48347.

## File Locations

| Component | Path |
|-----------|------|
| darkfid binary | `target/release/darkfid` |
| drk binary | `target/release/drk` |
| drk config | `bin/drk/drk_config.toml` |
| localnet config | `contrib/localnet/darkfid-single-node/darkfid.toml` |
| localnet drk config | `contrib/localnet/darkfid-single-node/drk.toml` |

## Related Documentation

- [Local Devnet Setup](../localnet-dev.md) - More details on localnet mining
- [Node Setup](../testnet/node.md) - DarkFi node configuration
- [Deploy Tutorial](../../learn/dchat/deployment/deploy.md) - Contract deployment guide