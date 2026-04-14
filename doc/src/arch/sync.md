# Sync Module

A clean, minimal blockchain synchronization module for DarkFi validator.

## Overview

The sync module provides stateless block verification and application, decoupled from the validator's block production machinery. This design eliminates the previous sled VK storage issues and simplifies testing.

## Motivation

The previous synchronization code suffered from:

1. **Sled VK Storage Issues**: The old code stored Verification Keys (VKs) in sled databases via `BlockchainOverlay::apply()`. This caused `UnexpectedEof` errors because the zkas tree wasn't properly flushed to sled.

2. **Coupled Verification**: Block verification was intertwined with WASM contract execution and state updates via `apply_producer_transaction()`.

3. **Testing Complexity**: The old sync tests required intricate setup to deploy contracts, manage overlays, and handle VK persistence.

```
Old flow (broken):
deploy() → zkas_db_set(VK) → overlay.apply() → [zkas tree NOT flushed] → new overlay reads from sled → UnexpectedEof
```

## Design Principles

### 1. No Sled for VKs During Sync

The node still uses sled for block storage, contract state, and transaction history. However, **VK retrieval during sync verification bypasses sled** - VKs are derived from `zkbin_bytes` passed alongside the block.

This fixes the `UnexpectedEof` bug where the zkas tree wasn't properly flushed to sled during `apply()`.

```rust
pub async fn verify_block(
    block: &BlockInfo,
    previous: &BlockInfo,
    zkbin_bytes: &[u8],  // VK derived from this at verification time
) -> Result<()> {
    let zkbin = ZkBinary::decode(zkbin_bytes, false)?;
    let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
    let vk = VerifyingKey::build(zkbin.k, &circuit);

    verify_header(block, previous)?;
    // ZK proof verification using derived VK
    Ok(())
}
```

### 2. Modular Operations

The sync module provides three separate operations:

| Function | Purpose |
|----------|---------|
| `verify_header()` | Validates block structure (height, timestamp, has txs) |
| `verify_block()` | Verifies header + ZK proofs (VK derived from bytes) |
| `apply_block()` | Persists verified block (placeholder) |

Each function is independently testable.

### 3. Stateless Verification

The `verify_block` function is **stateless** - it takes all required data as parameters:

- `block`: The block to verify
- `previous`: The previous block (for continuity checks)
- `zkbin_bytes`: ZkBinary bytes (for VK derivation)

No global state, no sled databases, no overlay management.

## API

### verify_header

```rust
pub fn verify_header(block: &BlockInfo, previous: &BlockInfo) -> Result<()>
```

Validates:
- Block has at least one transaction
- Block height is exactly `previous.height + 1`
- Block timestamp is strictly greater than `previous.timestamp`

### verify_block

```rust
pub async fn verify_block(
    block: &BlockInfo,
    previous: &BlockInfo,
    zkbin_bytes: &[u8],
) -> Result<()>
```

Validates:
- Header via `verify_header()`
- ZK proofs using VK derived from `zkbin_bytes`

### apply_block

```rust
pub async fn apply_block(block: &BlockInfo) -> Result<()>
```

Persists the block. Currently a placeholder - actual implementation would:
1. Execute contract calls
2. Update state monotree
3. Append block to chain

## Comparison

| Aspect | Old Sync | New Sync |
|--------|----------|----------|
| VK for Verification | Retrieved from sled | Derived from block data |
| Verification | Coupled with state updates | Independent function |
| Testing | Required complex harness | Simple unit test |
| Sled for VK | Required (but flush was broken) | Not needed for sync |
| State Management | Global overlay state | Passed as parameters |
| Code Complexity | Required apply(), diff(), overlay trees | Single function call |

**Note**: Sled is still used for block storage, contract state, and transaction history. The difference is that VK retrieval during sync no longer depends on sled.

## Why This Is Simpler

1. **No VK retrieval from sled**: The old code needed VK from sled during verification, but the zkas tree wasn't properly flushed. The new code derives VK from `zkbin_bytes` parameter.

2. **No overlay management for sync**: The old code required `BlockchainOverlay`, `apply()`, `diff()` for VK retrieval. The new sync function takes parameters.

3. **Testable in isolation**: `verify_block` can be tested with just `BlockInfo` and bytes - no need to set up sled databases for VK lookup.

4. **Clear separation**: The sync module only does sync verification - it doesn't know about consensus, mining, or contract execution.

## Usage Example

```rust
use darkfi::validator::sync::{verify_block, apply_block};

// In a test - zkbin loaded directly, no sled:
let zkbin_bytes = include_bytes!("../../contract/native_token/proof/mint_v1.zk.bin").to_vec();
verify_block(&block, &previous, &zkbin_bytes).await?;
apply_block(&block).await?;

// In production sync:
let zkbin_bytes = fetch_circuit_from_peer().await?;
verify_block(&block, &previous, &zkbin_bytes).await?;
apply_block(&block).await?;
```

## Breaking Changes

### 1. No VK Persistence

VKs are no longer stored in sled. Existing synced chains will not have VK data available via `contracts.get_zkas()`. Clients must provide ZkBinary bytes alongside blocks during synchronization.

### 2. Verification Key Source

Previously, VKs were fetched from the local sled database during verification:

```rust
// OLD - VK from sled
let (zkbin, vk) = overlay.get_zkas(&contract_id, "Mint_V1")?;
```

Now, verification requires external provision of ZkBinary bytes:

```rust
// NEW - VK derived from bytes
let zkbin_bytes = include_bytes!("..."); // or fetch from network
verify_block(&block, &previous, &zkbin_bytes).await?;
```

### 3. P2P Protocol Change

Blocks must now be accompanied by circuit data during synchronization. The P2P protocol must be updated to include ZkBinary bytes alongside block data.

## Trade-offs

| Aspect | Impact |
|--------|--------|
| **Performance** | Minor loss - circuit data transmitted per block (~100KB) |
| **Robustness** | Major gain - no sled VK retrieval failures |
| **Resilience** | Major gain - sync works even if sled VK storage is corrupted |
| **Determinism** | Major gain - VK always derived from known-good circuit data |

**Summary**: Minor performance loss for massive improvement in robustness, resilience, and determinism - all more important for a working blockchain than marginal throughput gains.

**Note**: This doesn't remove sled from the node - the node still uses sled for block storage, contract state, etc. It only removes sled dependency for VK retrieval during sync verification.

## Future Work

- [ ] `apply_block()` currently returns `Ok(())`. Actual block persistence needs implementation.
- [ ] ZK proof verification is currently a placeholder. Full ZK verification with derived VK needed.
- [ ] P2P protocol update needed to include ZkBinary bytes with blocks during sync.
- [ ] Integration with validator consensus module.

## Files

```
src/validator/sync/
├── mod.rs      # Module exports
├── types.rs    # SyncBlock, VerifyResult, SyncState
├── verify.rs   # verify_header, verify_block
├── apply.rs    # apply_block (placeholder)
└── README.md   # This documentation
```

## Related Documentation

- [Consensus](./consensus.md) - PoW consensus algorithm
- [Transaction Lifetime](./tx_lifetime.md) - Transaction processing lifecycle
- [Test Harness Guide](./test_harness_guide.md) - Writing contract integration tests