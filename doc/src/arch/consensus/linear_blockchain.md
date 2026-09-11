# Linear Blockchain Architecture

> **Note:** The linear blockchain runs on a single `CChainState`
> (`src/linear/src/chain_state.rs`). The architecture described here (TxBackend
> per-transaction execution, overlay, uncle-merkle) maps to that type.

The linear blockchain is DarkWow's consensus architecture using **Uncle Merkle consensus** with **RandomX proof-of-work** — a deterministic design where the canonical chain with the most accumulated work obligates offering uncle chains a one-time option to form a side chain and share the PoW reward.

## Proof-of-Work: RandomX

Linear uses **RandomX** (same as main DarkWow) for block hashing. This enables external miners (like xmrig) to connect via the stratum protocol.

### RandomX Key Rotation

Each block's header contains a `randomx_key` derived from the block height:

```rust
pub fn derive_key_from_height(height: BlockHeight) -> [u8; 32] {
    *blake3::hash(&height.to_le_bytes()).as_bytes()
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
    pub pin_accepted: bool,         // Uncle chain accepts (one-time decision)
    pub pin_confirmed: BlockReward, // base / 2^depth — 50% at d1, 25% at d2...
}
```

Depth is not stored — it is derived on demand via
`UncleBlock::depth_for(current_height, uncle_height)`, clamped to
`MAX_UNCLE_DEPTH` (6).

### UncleProof Structure

When an uncle is referenced in a canonical block, an **UncleProof** is constructed for stateless verification:

```rust
pub struct UncleProof {
    pub header: BlockHeader,         // Uncle's header (includes PoW)
    pub pow_hash: [u8; 32],          // RandomX PoW hash computed from header
    pub merkle_path: Vec<[u8; 32]>,  // Merkle proof path to uncle root
    pub position: u32,               // Uncle's position in merkle tree
    #[cfg(feature = "sharding")]
    pub state_root: Option<[u8; 32]>, // shard state root (shard uncle)
    #[cfg(feature = "sharding")]
    pub shard_id: Option<[u8; 32]>,   // shard identifier
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

Rewards are distributed between canonical miner and uncle miners by
**subtractive mass balance** (see [uncle_merkle.md](uncle_merkle.md)):

| Component | Formula |
|-----------|---------|
| Canonical reward | `base_reward − Σ pin_confirmed_i` (accepted uncles only) |
| Uncle reward at depth `d` | `pin_confirmed = base / 2^d` |
| Max depth | `MAX_UNCLE_DEPTH = 6` |

**Invariant:** `canonical_reward + Σ pin_confirmed_i == base_reward` (exactly
100%). The `total_reward` field in the canonical block header holds the
**canonical miner's effective reward** (`base_reward − Σ pin_confirmed_i`),
NOT the total emitted. The `total_reward + Σ pin_confirmed_i == base_reward`
invariant is enforced by `verify_uncle_split()`.

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

## Confirmation Model

DarkWow does not implement depth-based confirmation: no code reads a
block-depth threshold. Confirmation comes from two mechanisms, per
[consensus.md §Fork Choice Rule](consensus.md#fork-choice-rule):

1. **Finality anchors** — a block carrying an active Caribina (Arweave) or
   Monero finality anchor SHALL NOT be displaced by reorg (the finality guard
   inside `detect_reorg`).
2. **Heaviest-chain fork choice** — the canonical chain is the one with the
   most accumulated work (Bitcoin chainwork, `u32::MAX / target` per block).
   A displaced block is not discarded but reused as an uncle with a partial
   pin reward.

Until anchored, a block is probabilistic: a competing chain carrying strictly
more accumulated work can displace it (1-deep on the broadcast path,
general-depth on the sync path). An anchor makes displacement past the
anchored block cryptographically infeasible.

## RPC Endpoint

The `blockchain.get_block_linear` RPC endpoint returns wallet-compatible blocks:

```
Request:  {"jsonrpc": "2.0", "method": "blockchain.get_block_linear", "params": [height], "id": 1}
Response: {"jsonrpc": "2.0", "result": "base64encodedLinearBlockAdapter", "id": 1}
```

## ZK Verification

ZK proof verification follows a two-point model ([mempool.md §4](../mempool.md)):

1. **dwowd validates all proofs** at mempool admission and block acceptance —
   the first verification point, preventing invalid transactions from propagating.
2. The wallet scanner **independently verifies ZK proofs** — it does not trust
   dwowd's validation. This is the second verification point, ensuring a syncing
   node validates every transaction it did not admit itself.
3. Scanner verification uses the verifying key declared by the contract's
   `metadata()` ABI ([type-system.md §8.2](../type-system.md)).

Both verification points SHALL use the same verifying keys and SHALL produce
the same acceptance decision for the same transaction. This is the
Authenticated-Pool invariant ([mempool.md §1](../mempool.md)).

## Limitations

1. **DarkLeaf tree**: Contract call hierarchy uses DarkLeaf at execution time (`src/linear/src/execution.rs`) — child call traversal is supported.
2. **Simplified state**: No overlay/rollback system means deterministic, reproducible state — a design feature, not a limitation.