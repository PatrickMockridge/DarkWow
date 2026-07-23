# Sync Module

A clean, minimal blockchain synchronization module for DarkWow's linear blockchain.

> **Architecture note**: The sync module previously lived at `src/validator/sync/`.
> It has been migrated to the linear blockchain architecture: `CChainState`
> (`src/linear/src/chain_state.rs`) manages block connection and state,
> `bin/dwowd/src/block_acceptor.rs` handles block acceptance, and
> `bin/dwowd/src/proto/linear_broadcast.rs` handles P2P block propagation.
> Some code examples below reference the pre-migration API and should be
> read as conceptual patterns rather than copy-pasteable code.

## Overview

The sync module provides stateless block verification and application, decoupled from the validator's block production machinery. This design eliminates the previous sled VK storage issues and provides deterministic ZK proof verification.

## Motivation

The previous synchronization code suffered from:

1. **Sled VK Storage Issues**: The old code stored Verification Keys (VKs) in sled databases via `BlockchainOverlay::apply()`. This caused `UnexpectedEof` errors because the zkas tree wasn't properly flushed to sled.

2. **Coupled Verification**: Block verification was intertwined with WASM contract execution and state updates via `apply_producer_transaction()`.

3. **Non-deterministic Verification**: VKs were retrieved from sled which could be missing or corrupted, making verification unreliable during sync.

## Design Principles

### 1. Pure ZK Verification

ZK proof verification is completely stateless and deterministic using the `verify_zkp` function:

```rust
pub fn verify_zkp(
    proof: &Proof,
    zkbin_bytes: &[u8],
    instances: &[pallas::Base],
) -> ZkVerifyResult
```

- Same inputs always produce same output
- No sled, no WASM, no side effects
- VK is derived fresh at verification time from `zkbin_bytes`

### 2. ZkBinEntry for Sync

The `ZkBinEntry` type carries all data needed for sync verification:

```rust
pub type ZkBinEntry = (ContractId, String, Vec<u8>, Vec<pallas::Base>);
//                       contract_id,  zkas_ns,  zkbin_bytes,  instances
```

This format is used throughout the sync flow:
- `ExtendedProposalMessage` contains `Vec<ZkBinEntry>` for P2P transmission
- `BlockInfo.zkbin_data` stores zkbin entries for verification
- `Proposal.zkbin_data` carries data through the consensus layer

### 3. No Sled for VKs During Sync

VK retrieval during sync verification **bypasses sled completely** - VKs are derived from `zkbin_bytes` passed alongside the block.

This fixes the `UnexpectedEof` bug where the zkas tree wasn't properly flushed to sled during `apply()`.

## Architecture

### Data Flow

```
Block Created
    ↓
BlockInfo.zkbin_data populated with (contract_id, zkas_ns, zkbin_bytes, instances)
    ↓
ExtendedProposalMessage broadcast via P2P
    ↓
handle_receive_proposal extracts zkbin_data
    ↓
sync::verify_block(&block, &previous, &zkbin_data)
    ↓
verify_zkp(proof, zkbin_bytes, instances) for each proof
    ↓
chain_state.connect_block(&block).await?;
```

### Key Components

| Component | File | Purpose |
|-----------|------|---------|
| `verify_zkp` | `src/zk/verifier.rs` | Pure ZK verification function |
| `ZkVerifyResult` | `src/zk/verifier.rs` | Verification result enum |
| `ZkBinEntry` | `src/linear/src/chain_state.rs` | `(ContractId, String, Vec<u8>, Vec<pallas::Base>)` |
| `ExtendedProposalMessage` | `bin/dwowd/src/proto/protocol_proposal.rs` | P2P message with zkbin_data |
| `verify_block` | `bin/dwowd/src/block_acceptor.rs` | Verifies header + ZK proofs |
| `handle_receive_proposal` | `bin/dwowd/src/proto/protocol_proposal.rs` | Calls verify_block before append |

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
    zkbin_data: &[ZkBinEntry],
) -> Result<()>
```

Validates:
- Header via `verify_header()`
- ZK proofs using VK derived from `zkbin_data`

For each transaction call:
1. Match proof to `(zkbin_bytes, instances)` using contract_id + zkas_ns lookup
2. Call `verify_zkp(proof, zkbin_bytes, instances)`
3. If verification fails, return error

### accept_block / connect_block

Block application uses two functions:

- `accept_block(chain_state, block, uncles, vm, current_height, target)` in
  `bin/dwowd/src/block_acceptor.rs` — full block acceptance with WASM execution,
  PoW validation, and atomic commit. Used by the production path (miner RPC,
  stratum, sync).
- `connect_block(block)` in `src/linear/src/chain_state.rs` — inserts an
  already-validated block into the chain state (height, coin/nullifier sets,
  uncle linking). Called internally by `accept_block` after WASM execution
  completes.

## Types

### ZkBinEntry

```rust
pub type ZkBinEntry = (ContractId, String, Vec<u8>, Vec<pallas::Base>);
//                       contract_id,  zkas_ns,  zkbin_bytes,  instances
```

- `contract_id`: Identifies the contract
- `zkas_ns`: Namespace of the ZK circuit (e.g., "Mint_V1")
- `zkbin_bytes`: Compiled circuit binary
- `instances`: Public inputs for proof verification

### ZkVerifyResult

```rust
pub enum ZkVerifyResult {
    Ok,
    InvalidProof,
    InvalidVk,
}
```

Result of pure ZK verification.

## P2P Protocol

### ExtendedProposalMessage

```rust
pub struct ExtendedProposalMessage {
    pub proposal: Proposal,
    pub zkbin_data: Vec<ZkBinEntry>,
}
```

Replaces `ProposalMessage` for sync. The `zkbin_data` field carries all circuit data needed for stateless verification.

### handle_receive_proposal Flow

```rust
// Get previous block for verification
let previous_block = chain_state.get_block(previous_hash)?.ok_or(Error::BlockNotFound)?;;

// Verify ZK proofs statelessly using zkbin_data
sync::verify_block(&proposal.block, &previous, &zkbin_data).await?;

// Only then connect block to chain state
chain_state.connect_block(&block).await?;
```

## Comparison

| Aspect | Old Sync | New Sync |
|--------|----------|----------|
| VK for Verification | Retrieved from sled | Derived from zkbin_bytes |
| Verification | Coupled with state updates | Independent function |
| Testing | Required complex harness | Simple unit test |
| Sled for VK | Required (but flush was broken) | Not needed for sync |
| State Management | Global overlay state | Passed as parameters |
| Code Complexity | Required apply(), diff(), overlay trees | Single function call |
| ZK Verification | Placeholder | Full verification via verify_zkp |

## Why This Is Better

1. **No sled VK retrieval**: The old code needed VK from sled during verification, but the zkas tree wasn't properly flushed. The new code derives VK from `zkbin_bytes` parameter.

2. **Deterministic verification**: Same `zkbin_bytes` + `instances` always produces same result. No dependency on sled state.

3. **Testable in isolation**: `verify_block` can be tested with just `BlockInfo` and `zkbin_data` - no need to set up sled databases for VK lookup.

4. **Clear separation**: The sync module only does sync verification - it doesn't know about consensus, mining, or contract execution.

5. **P2P Integration**: `ExtendedProposalMessage` carries zkbin_data over the wire, enabling stateless verification on the receiving end.

## Usage Example

### Test with Real ZK Verification

```rust
// Generate block with PoW reward transaction
let debris = PoWRewardCallBuilder { ... }.build()?;

// Extract public inputs for verification
let public_inputs = vec![
    debris.params.output.coin.inner(),
    value_coords.x(),
    value_coords.y(),
    debris.params.output.token_commit,
];

// Attach zkbin_data to block
block.zkbin_data = vec![(
    *NATIVE_TOKEN_CONTRACT_ID,
    "Mint_V1".to_string(),
    zkbin_bytes,
    public_inputs,
)];

// Verify using sync module - VK derived from zkbin_bytes
verify_block(&block, &previous, &block.zkbin_data).await?;
```

### Production Sync Flow

```rust
// Receive ExtendedProposalMessage via P2P
let (channel, message) = handler.receiver.recv().await;
let proposal = message.proposal;
let zkbin_data = message.zkbin_data;

// Get previous block
let previous_block = chain_state.get_block(block.header.height - 1)?.ok_or(Error::BlockNotFound)?;

// Verify ZK proofs statelessly
sync::verify_block(&proposal.block, &previous_block, &zkbin_data).await?;

// Only then append
chain_state.connect_block(&block).await?;.await?;
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

Now, verification requires zkbin_data:

```rust
// NEW - VK derived from zkbin_data
sync::verify_block(&block, &previous, &block.zkbin_data).await?;
```

### 3. P2P Protocol Change

Blocks must now be accompanied by circuit data during synchronization via `ExtendedProposalMessage`. The old `ProposalMessage` does not carry zkbin_data.

## Trade-offs

| Aspect | Impact |
|--------|--------|
| **Performance** | Minor loss - zkbin_data transmitted per block (~100KB) |
| **Robustness** | Major gain - no sled VK retrieval failures |
| **Resilience** | Major gain - sync works even if sled VK storage is corrupted |
| **Determinism** | Major gain - VK always derived from known-good circuit data |

**Summary**: Minor performance loss for massive improvement in robustness, resilience, and determinism - all more important for a working blockchain than marginal throughput gains.

**Note**: This doesn't remove sled from the node - the node still uses sled for block storage, contract state, etc. It only removes sled dependency for VK retrieval during sync verification.

## Implementation Status

- [x] `verify_zkp` - Pure ZK verification function
- [x] `ZkVerifyResult` - Verification result enum
- [x] `ZkBinEntry` - Type for sync verification data
- [x] `verify_block` - Verifies header + ZK proofs
- [x] `ExtendedProposalMessage` - P2P message with zkbin_data
- [x] `handle_receive_proposal` - Calls verify_block before append
- [x] `accept_block` + `connect_block` — production block application path

## Files

```
src/zk/
└── verifier.rs          # Pure ZK verification (verify_zkp, ZkVerifyResult)

bin/dwowd/src/proto/
└── protocol_proposal.rs  # ExtendedProposalMessage, handle_receive_proposal

bin/dwowd/src/tests/
├── genesis.rs           # GenesisHarness - baseline chain for WASM contract tests
├── sync_simple.rs       # Basic sync test
└── sync_native.rs       # Full ZK verification test
```

## Gossip Propagation

Block sync messages are carried over the P2P network using structured fan-out
gossip. The `broadcast_block()` function at `linear_broadcast.rs:206-256`
implements the ρ-calculus `GossipStructured(b)` process (see
[Type System §10.2](type-system.md#102-blockchain-path--structured-gossip)):
each block is relayed to `k = ⌈log₂(N)⌉` randomly selected peers, producing
O(log N) propagation rounds with O(k·N) total messages.

Sync-specific messages (`GetTip`, `GetBlocks`) use point-to-point P2P channels
and do not require structured gossip. Block broadcast is the only sync message
path that uses fan-out propagation.

See [P2P Network](net/p2p-network.md#structured-gossip) and
[Observer](observer.md) (relay behavior).

## Related Documentation

- [ZK Verification](./zk/zk_verification.md) - Detailed ZK verifier design
- [Genesis Harness](../dev/testing/level-1-lightweight.md) - Baseline chain setup for WASM contract tests
- [Consensus](./consensus/consensus.md) - PoW consensus algorithm
- [Transaction Lifetime](./sc/tx-lifetime.md) - Transaction processing lifecycle