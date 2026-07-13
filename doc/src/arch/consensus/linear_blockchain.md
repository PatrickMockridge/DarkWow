# Linear Blockchain Architecture

> **Note:** This document describes the Uncle Merkle architecture which has been
> refactored (commit `597691582`). The dual-LinearBlockchain no longer exists;
> its replacement is a single `CChainState` (`src/linear/src/chain_state.rs`).
> The conceptual architecture described here (TxBackend, overlay, uncle-merkle)
> remains correct; only the type and file layout changed.

The linear blockchain is DarkWow's consensus architecture using **Uncle Merkle consensus** with **RandomX proof-of-work**. It replaces upstream's overlay/diff architecture with a deterministic design where the canonical chain with the most accumulated work obligates offering uncle chains a one-time option to form a side chain and share the PoW reward.

## Overview

The linear blockchain differs from the original DarkWow consensus in several key ways:

| Aspect | Original (Fork/Overlay) | Linear (Uncle Merkle) |
|--------|------------------------|----------------------|
| State management | Overlay + diffs + rollback | Plain sled |
| Fork resolution | Implicit competition | Explicit uncle reference |
| Mining risk | All-or-nothing | Bounded (uncle gets partial) |
| Verification | Heavy WASM + sled lookups | Merkle proof only |
| Determinism | Non-deterministic in time | Fully deterministic |
| Complexity | High | Low |

## Proof-of-Work: RandomX

Linear uses **RandomX** (same as main DarkWow) for block hashing. This enables external miners (like xmrig) to connect via the stratum protocol.

### RandomX Key Rotation

Each block's header contains a `randomx_key` derived from the block height:

```rust
pub fn derive_key_from_height(height: u64) -> [u8; 32] {
    let height_bytes = height.to_le_bytes();
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&height_bytes);
    key
}
```

Miners use this key to create a RandomX VM for hashing blocks. The key changes every block to prevent pre-computation attacks.

### Block Hashing

Block hashes are computed by passing the serialized header through RandomX:

```rust
impl Block {
    pub fn hash(&self, vm: &RandomXVM) -> blake3::Hash {
        let mut header_bytes = Vec::new();
        self.header.encode(&mut header_bytes).unwrap();
        let rx_hash = vm.calculate_hash(&header_bytes).expect("RandomX hash failed");
        // Use first 32 bytes as blake3-compatible hash
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&rx_hash[..32]);
        blake3::Hash::from_bytes(hash_bytes)
    }
}
```

## Uncle Block Architecture

When a block is not accepted into the canonical chain, it can still be referenced as an **uncle block** by subsequent canonical blocks. This provides mining rewards to miners who otherwise would have wasted their work.

### UncleBlock Structure

```rust
pub struct UncleBlock {
    pub header: BlockHeader,        // Has its own PoW
    pub transactions: Vec<Transaction>,
    pub depth: u8,                  // 1 = directly referenced, 2 = depth-1, etc.
    pub pin_offered: bool,          // Canonical chain offers pin
    pub pin_accepted: bool,          // Uncle chain accepts (one-time decision)
    pub pin_reward: u64,             // Computed from depth: 50% at d1, 25% at d2...
}
```

### UncleProof Structure

When an uncle is referenced in a canonical block, an **UncleProof** is constructed for stateless verification:

```rust
pub struct UncleProof {
    pub header: BlockHeader,         // Uncle's header (includes PoW)
    pub pow_hash: [u8; 32],          // RandomX PoW hash computed from header
    pub merkle_path: Vec<[u8; 32]>,  // Merkle proof path to uncle root
    pub position: u32,                // Uncle's position in merkle tree
    pub depth: u8,                   // Depth for reward calculation
}
```

## Uncle Proof Verification

The critical security property: **UncleProof must bind the RandomX PoW to the proof structure**, making it impossible to submit fake uncle proofs without doing the actual RandomX work.

### Verification Steps

When verifying an `UncleProof`:

1. **PoW Hash Verification**: Re-compute the RandomX PoW hash from the proof's header using the header's `randomx_key`. Compare against the stored `pow_hash`.

2. **Target Check**: Verify the PoW hash meets the target (`hash_u32 <= target`). Higher target = easier mining.

3. **Merkle Proof**: Verify the header is included in the uncle merkle tree rooted at the canonical block's `uncle_merkle_root`.

```rust
pub fn verify_uncle_proof(
    uncle: &UncleProof,
    merkle_root: &[u8; 32],
    _vm: &randomx::RandomXVM,
    target: u32,
) -> bool {
    // Step 1: Verify pow_hash matches re-computed hash from header
    // We create a VM with the uncle's specific randomx_key
    let header_bytes = serde_json::to_vec(&uncle.header).unwrap();
    let cache = randomx::RandomXCache::new(flags, &uncle.header.randomx_key)?;
    let verify_vm = randomx::RandomXVM::new(flags, Some(cache), None)?;
    let rx_hash = verify_vm.calculate_hash(&header_bytes)?;
    let computed_pow_hash: [u8; 32] = rx_hash[..32];

    if computed_pow_hash != uncle.pow_hash {
        return false;  // PoW hash mismatch
    }

    // Step 2: Verify pow_hash meets target (hash_u32 <= target)
    let hash_u32 = u32::from_le_bytes(computed_pow_hash[0..4].try_into().unwrap());
    if hash_u32 > target {
        return false;  // Target not met
    }

    // Step 3: Verify merkle proof against uncle_merkle_root
    // ... merkle verification ...
}
```

### Uncle Merkle Tree Construction

The canonical block's `uncle_merkle_root` is built from uncle proofs:

```rust
pub fn build_uncle_merkle(uncles: &[UncleBlock], _vm: &RandomXVM) -> ([u8; 32], Vec<UncleProof>) {
    // 1. Compute pow_hash for each uncle using their randomx_key
    // 2. Build merkle tree using blake3 for structure (not PoW)
    // 3. Return root and proofs with position information
}
```

The merkle tree itself uses blake3 for structure (for efficient verification), while RandomX provides the actual PoW security.

## Reward Distribution

Rewards are distributed between canonical miner and uncle miners:

| Component | Formula |
|-----------|---------|
| Canonical reward | Full block reward |
| Uncle reward at depth 1 | 50% of canonical (pin_reward = base / 2^1) |
| Uncle reward at depth 2 | 25% of canonical (pin_reward = base / 2^2) |
| Max depth | 6 (rewards become negligible below this) |

The `total_reward` in the canonical block header accounts for all rewards (canonical + uncle shares).

## Transaction-First Block Construction

The linear blockchain is built around a fundamental design principle: **the
individual transaction is the primitive unit of execution.** Blocks and
uncle-merkle side chains are constructed *from* transaction results, not the
other way around.

### Architecture

```
TxBackend (per-transaction state access)
├── overlay: SledTreeOverlay     ← in-memory buffer, no sled writes during execution
└── store: Arc<LinearStore>      ← read-only contract data lookups

CChainState (pure coordinator — never enters execution path)
├── Validates PoW, merkle roots, uncle proofs
├── Dispatches transactions sequentially through TxBackend
├── Merges per-tx diffs deterministically (sort by tx hash, canonical-first)
└── Commits atomically via single sled::Batch
```

### Key principles

1. **Transaction is the fundamental primitive.** Each contract call executes with
   its own `TxBackend` — a minimal struct containing only a `SledTreeOverlay`
   (in-memory state buffer) and an `Arc<LinearStore>` (read-only contract data).
   The `CChainState` coordinator never enters the WASM execution path.

2. **`sled_overlay` is an atomicity device, not a parallelism mechanism.**
   `SledTreeOverlay::checkpoint()` and `revert_to_checkpoint()` ensure per-call
   atomicity — either all state writes from a contract call commit, or none do.
   All state changes are buffered in-memory and committed in a single
   `sled::Batch` at the end of block execution.

3. **Block as lightweight wrapper.** Blocks are assembled *after* execution:
   transaction results → merkle tree → block header. The block is confirmation +
   broadcast + acceptance metadata — not an execution primitive.

4. **Uncle-merkle cascades naturally.** Uncle blocks are alternative merkle trees
   of transactions. Each uncle's transactions execute with the same overlay
   isolation as canonical transactions. After all results are collected,
   canonical writes take precedence on key conflicts (uncle diffs subtract the
   canonical total before merging).

5. **Deterministic merge.** Results are sorted by transaction hash bytes.
   Canonical results are applied first, then uncle results (with canonical
   conflicts subtracted). This guarantees deterministic state regardless of
   execution order.

### Execution model

Transactions execute sequentially on the calling thread, matching upstream
DarkFi's proven pattern. The architecture supports parallelism — each
transaction has independent state scope via its own `TxBackend` — but wasmer's
current concurrency model (cross-Engine `Module` reuse) is not safe for
concurrent instantiation. When wasmer matures, parallelism is a one-line change:
wrap the execution loop in `thread::spawn`.

## WASM Contract Model

The linear blockchain executes smart contracts written in WebAssembly (WASM).

### Contract Lifecycle

Each WASM contract implements four lifecycle functions:

```rust
pub enum ContractSection {
    Deploy,   // __initialize - setup trees, store WASM bincode
    Exec,     // __entrypoint - process instructions
    Update,   // __update - apply state changes
    Metadata, // __metadata - extract public inputs for ZK proofs
    Null,
}
```

1. **`__initialize`** (Deploy phase): Sets up contract state trees and stores WASM binary
2. **`__metadata`**: Extracts public inputs and signature public keys for ZK proofs
3. **`__entrypoint`** (Exec phase): Verifies state transition and returns update buffer
4. **`__update`**: Applies state changes to persist modifications

### init_contract Convention

`deploy_contract()` in the daemon passes an empty payload (`&[]`) when
deploying contracts outside the Deployooor flow (e.g., during tests and
genesis initialization). Contracts MUST handle an empty `ix` byte slice in
`init_contract()` by falling back to sensible defaults derived from the
contract's own constants. When `ix` is non-empty, decode and use the
provided parameters as normal (the production Deployooor path).

25 of 28 contracts already follow this convention. The contract author
should verify any new `init_contract` against the survey below.

### Host Functions

WASM contracts can access these host functions:

| Function | Purpose |
|----------|---------|
| `db_init_`, `db_lookup_`, `db_get_`, `db_set_`, `db_del_` | State operations |
| `zkas_db_set_` | Store ZK circuit binaries |
| `merkle_add_`, `sparse_merkle_insert_batch_` | Merkle tree operations |
| `get_tx_hash_`, `get_call_index_`, `get_verifying_block_height_` | Context queries |

### Gas Metering

The WASM runtime uses `Metering` middleware with `GAS_LIMIT = 400_000_000` points.

## LinearBlockAdapter

The wallet scanner cannot directly process linear blocks because the types differ:

| Aspect | Regular Blockchain | Linear Blockchain |
|--------|------------------|------------------|
| Block type | `BlockInfo` | `LinearBlock` |
| Header height | `u32` | `u64` |
| Contract ID | `ContractId` | `Hash` (blake3::Hash) |
| Transaction calls | `Vec<DarkLeaf<ContractCall>>` | `Vec<ContractCall>` |

The `LinearBlockAdapter` translation layer bridges this gap:

```rust
struct LinearBlockAdapter {
    header: LinearHeaderAdapter,      // Mirrors Header
    txs: Vec<LinearTransactionAdapter>, // Mirrors Transaction
    signature: Signature,            // Linear has no block signatures (uses dummy)
    zkbin_data: Vec<(ContractId, String, Vec<u8>, Vec<pallas::Base>)>,
}

struct LinearHeaderAdapter {
    version: u8,
    previous: [u8; 32],
    height: u32,                      // Truncated from u64
    nonce: u32,
    timestamp: u64,
    transactions_root: MerkleNode,
    state_root: [u8; 32],
    pow_data: PowData,                 // Always PowData::DarkWow
    uncle_merkle_root: [u8; 32],
    total_reward: u64,
}
```

## Confirmation Model

The linear blockchain uses **depth-based confirmation** — substantially simpler
than the fork-based model:

1. Blocks are appended sequentially to a single canonical chain. There are no
   competing forks — by design, there is only one valid next block at any height.
2. A block is **confirmed** when a configurable number of subsequent blocks
   have built on top of it. This depth is set by the `threshold` parameter in
   `dwowd_config.toml` (default: 3 for `darkwow-testnet`, 1 for `darkwow-devnet`).
3. With a 120-second block time and `threshold = 3`, finality is reached in
   approximately 6 minutes.
4. **No fork choice** is needed — the chain is linear by construction. There is
   no `best_fork_index()`, no rank competition, and no overlay/diff system.

### Comparison with Fork-Based Consensus

| Aspect | Fork/Overlay (DAG) | Linear (Uncle Merkle) |
|--------|-------------------|----------------------|
| Chain structure | DAG of competing forks | Single linear chain |
| Confirmation | Fork length > threshold + no competing fork with same rank | Block depth > threshold |
| Fork resolution | Ranking (`targets_rank`, `hashes_rank`) | Not applicable (no forks) |
| State model | Overlay + diffs + rollback | Plain sled (final writes) |
| Uncle handling | Implicit competition | Explicit reference + pin reward |

The `threshold` parameter serves the same semantic role in both models (minimum
depth before a block is considered final), but the linear model has no fork
ranking or competition logic.

## RPC Endpoint

The `blockchain.get_block_linear` RPC endpoint returns wallet-compatible blocks:

```
Request:  {"jsonrpc": "2.0", "method": "blockchain.get_block_linear", "params": [height], "id": 1}
Response: {"jsonrpc": "2.0", "result": "base64encodedLinearBlockAdapter", "id": 1}
```

## ZK Verification

ZK proof verification in the linear blockchain differs from the original:

1. **dwowd validates all proofs** before including transactions in blocks
2. The wallet scanner **does not verify proofs** - it trusts that dwowd has validated them
3. Scanner's only job is **note decryption** to detect wallet-owned coins

This is a deliberate design choice that simplifies the wallet scanner significantly.

## Limitations

1. **No DarkLeaf children_indexes**: Linear `ContractCall` lacks the `children_indexes` field, so DAO child call traversal is not possible
2. **No ZK verification in scanner**: Scanner trusts dwowd's validation
3. **Simplied state**: No overlay/rollback system means simpler but less powerful state management