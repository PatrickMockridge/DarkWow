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
- RPC endpoint: `127.0.0.1:8548`
- Config: `contrib/localnet/darkfid-single-node/darkfid.toml`
- `pow_fixed_difficulty=1` makes mining fast for testing

### drk (CLI wallet)
```bash
# Mine blocks to default wallet address
drk -n localnet mine

# Check balance
drk -n localnet wallet balance

# Scan blockchain for new coins
drk -n localnet scan
```

### drk_config.toml
```toml
[settings]
network = "localnet"  # Critical - enables mining
rpc_endpoint = "127.0.0.1:8548"
```

## Mining Details

- **Algorithm:** RandomX (rx/0)
- **Difficulty:** 1 (very low, shares found frequently)
- **Block reward:** 20 DARK per mined block
- **Blob structure:** 43 bytes = [2 byte padding][40 byte header with nonce at offset 39]
- **Target:** 8 bytes MSB of 32-byte target, padded with zeros to 32 bytes for comparison
- **Nonce:** 4 bytes at blob byte offset 39 (little-endian u32)

## Why Mining vs Faucet?

The `faucet.mint` RPC was broken (simulation errors). Mining provides a reliable way to fund the wallet for contract testing. Once faucet is fixed upstream, this mining approach can be abandoned.

## Troubleshooting

```bash
# If "Resource temporarily unavailable" error on wallet db:
# Kill any running drk processes
pkill -f "drk.*mine"

# Then retry wallet commands
drk -n localnet wallet balance
```

## File References

- `bin/darkfid/src/rpc/miner.rs` - darkfid stratum server implementation (RPC `miner.mine`)
- `bin/darkfid/src/lib.rs` - `DarkfiNode::is_localnet()` guard
- `bin/drk/src/main.rs` - `Mine` subcommand enum and handler
- `bin/drk/src/rpc.rs` - `miner_mine()` method with stratum protocol client
- `bin/drk/Cargo.toml` - Dependencies: darkfi with async-daemonize, rpc, randomx features
- `contrib/localnet/darkfid-single-node/darkfid.toml` - Localnet config