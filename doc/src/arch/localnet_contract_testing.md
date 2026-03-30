# Local DarkFi Smart Contract Testing

## Overview

This documents the current state of localnet smart contract testing in DarkFi. It is a **statement of fact** about what functionality is available, what is not, and the known issues affecting contract testing.

## Current State (2026-03-30)

Localnet smart contract testing is **partially functional** but has significant gaps due to:

1. **async-trait lifetime bug** - Prevents building integration tests from source (see [Async Serialization Lifetime Bug](./async_serial_lifetime_bug.md))
2. **Missing wallet commands** - The `drk wallet` subcommand was removed or not implemented in v0.5.0
3. **No test token minting** - No mechanism to obtain test DARK tokens on localnet

> **Conda Users**: If using conda environments, be aware that conda's Python may conflict with system Python packages. Consider using `conda deactivate` before running DarkFi binaries, or use a separate venv as described in [Using dnet](../learn/dchat/network-tools/using-dnet.md).

## What Works

### Binary Availability

Pre-built DarkFiMain binaries (v0.5.0) work and can be used for:
- Running a localnet validator node (`darkfid`)
- CLI wallet operations (`drk`)
- Mining via xmrig + p2pool (requires full Monero setup)
- Contract compilation to WASM (via `cargo build --target wasm32-unknown-unknown`)

Binaries location: `/home/patrick/DarkFiMain/darkfi/bin/`

### Localnet Node

A localnet validator node can be started with:

```bash
darkfid --config localnet.config.toml run
```

Configuration for localnet is in `/home/patrick/darkfi-testnet/`:

**localnet.config.toml** - Node configuration:
```toml
network = "localnet"
skip_sync = true
skip_fees = true
```

**drk.config.toml** - Wallet configuration:
```toml
network = "localnet"
endpoint = "tcp://127.0.0.1:28345"
wallet_pass = "test123"
```

### Contract WASM Compilation

Contracts compile successfully to WASM:
```bash
cd src/contract/baccarat
cargo build --target wasm32-unknown-unknown --release
```

Output: `target/wasm32-unknown-unknown/release/darkfi_baccarat_contract.wasm`

### Available drk Commands

The `drk` binary provides these subcommands:

```
drk alias          # Token alias management
drk contract       # Contract deployment (deploy, generate-deploy, list, lock)
drk token         # Token functionalities (freeze, generate-mint, import, list, mint)
```

#### Contract Deployment

Contract deployment syntax:
```
drk contract deploy <deploy-auth> <wasm-path> <deploy-ix>
```

Where:
- `<deploy-auth>` - Contract ID (deploy authority), generated via `drk contract generate-deploy`
- `<wasm-path>` - Path to compiled WASM file
- `<deploy-ix>` - Path to serialized deploy instruction

```bash
# Generate a deploy authority
drk contract generate-deploy

# Deploy a contract (syntax: drk contract deploy <auth> <wasm> <ix>)
drk contract deploy <authority-id> path/to/contract.wasm ./deploy_ix.bin
```

#### Token Operations

```bash
# Generate mint authority for custom token
drk token generate-mint

# Mint custom tokens (requires mint authority)
drk token mint <token-id> <amount> <recipient>

# List available mint authorities
drk token list
```

## What Doesn't Work

### drk wallet Commands

**The `drk wallet` subcommand does not exist in v0.5.0.**

This breaks:
- Wallet creation (`drk wallet create`)
- Token minting via mining (`drk wallet mine`)
- Balance checking (`drk wallet balance`)
- Coin listing (`drk wallet coins`)

These commands were documented in older DarkFi documentation but are not present in the current binary.

### Test Token Minting

**There is no way to obtain test DARK tokens on localnet with current tooling.**

The options that should exist but don't:
1. `drk wallet mine` - Does not exist
2. `drk wallet genesis-mint` - Does not exist
3. Direct DARK token faucet - Does not exist

The `Money::GenesisMintV1` contract function only works at block 0 (genesis), and is used internally by the test harness for setting up initial test conditions.

### Integration Tests

**Cannot compile due to async-trait bug.** See [Async Serialization Lifetime Bug](./async_serial_lifetime_bug.md) for details.

The test harness that should provide:
- Genesis minting for test setup
- Contract function execution in isolation
- State verification

...cannot be built because of the async-trait lifetime issue affecting all Rust versions.

## Architectural Connection to async-trait Issue

The async-trait bug and the missing wallet commands are related:

```
async-trait bug (blocks building from source)
    ↓
Cannot build darkfid/drk from source
    ↓
Must use pre-built DarkFiMain binaries
    ↓
Pre-built binaries don't have wallet commands
    ↓
No test token minting on localnet
```

The test harness (which would bypass the token issue) cannot be compiled due to the async-trait bug. If we could build from source, we could:
1. Potentially fix the async-trait issue
2. Access wallet commands that exist in source but not binaries

## Workarounds

### Option 1: Full Mining Stack

Set up Monero + p2pool + xmrig merge mining to earn DARK tokens:

1. Sync Monero testnet
2. Start p2pool with merge-mining enabled
3. Start xmrig to mine blocks
4. Earn block rewards (PoWRewardV1)

See: [Merge Mining](../testnet/merge-mining.md)

**Drawback**: Requires full Monero infrastructure, complex setup

### Option 2: Custom Token with Mint Authority

1. Create a custom token
2. Generate mint authority
3. Mint custom tokens for testing

**Drawback**: Doesn't help with DARK token needed for contract deployment fees

### Option 3: Wait for Tooling Fix

1. Wait for DarkFiMain to restore `wallet` commands
2. Or wait for async-trait fix to build from source

**Drawback**: Blocks contract testing indefinitely

## File Locations

| Component | Path |
|-----------|------|
| Localnet config | `/home/patrick/darkfi-testnet/localnet.config.toml` |
| Wallet config | `/home/patrick/darkfi-testnet/drk.config.toml` |
| DarkFiMain binaries | `/home/patrick/DarkFiMain/darkfi/bin/` |
| Pre-built darkfid | `/home/patrick/DarkFiMain/darkfi/bin/darkfid/darkfid` |
| Pre-built drk | `/home/patrick/DarkFiMain/darkfi/bin/drk/drk` |
| Baccarat WASM | `/home/patrick/Darkfi/darkfi/src/contract/baccarat/darkfi_baccarat_contract.wasm` |
| Async-trait issue | `/home/patrick/Darkfi/darkfi/doc/src/arch/async_serial_lifetime_bug.md` |

## Related Documentation

- [Async Serialization Lifetime Bug](./async_serial_lifetime_bug.md) - Integration test compilation failure
- [Test Harness Guide](./test_harness_guide.md) - Expected test harness functionality
- [Merge Mining](../testnet/merge-mining.md) - Full mining setup
- [Node Setup](../testnet/node.md) - DarkFi node configuration

## Summary

| Feature | Status | Notes |
|---------|--------|-------|
| Localnet validator | ✅ Works | With skip_fees=true |
| RPC connectivity | ✅ Works | tcp://127.0.0.1:28345 |
| Contract WASM build | ✅ Works | Via cargo |
| Contract deployment | ⚠️ Partial | Syntax differs from docs |
| Test token minting | ❌ Broken | No wallet mine command |
| Integration tests | ❌ Broken | async-trait bug |
| Custom token minting | ⚠️ Partial | Requires mint authority |
| Merge mining | ⚠️ Complex | Requires Monero infrastructure |
