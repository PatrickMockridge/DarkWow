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
    pub fn hash_with_vm(&self, vm: &RandomXVM) -> Result<blake3::Hash> {
        // Hashes the header's mining blob through RandomX; returns a Result,
        // not a panicking Hash (`block.rs:233-245`).
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
    pub merkle_path: Vec<[u8; 32]>,  // Merkle proof path from uncle to root
    pub position: u32,               // Uncle's position in merkle tree (leaf index)
}
```

**That is the whole struct.** It carried `header`, `pow_hash`, a `state_root` and more until
2026-10-12 (`F-356`); the real one is `src/linear/src/block.rs:297-302` and has two fields. The
removal is not incidental — the file's own comment above the struct records what carrying them cost:
a full RandomX cache+VM initialisation **per uncle**, purely to fill a field the verifier then
recomputed, which is 2N initialisations of the deliberately expensive step per block on the accept
path. The uncle's header travels in `UncleBlock`, which is what the verifier has; a proof does not
need to restate it — and the fields this section used to show (`header`, `pow_hash`, a `state_root`,
a `shard_id`) name four things the struct does not have and a `sharding` feature that is not in the
tree.

## Uncle Proof Verification

The critical security property: **UncleProof must bind the RandomX PoW to the proof structure**, making it impossible to submit fake uncle proofs without doing the actual RandomX work.

### Verification Steps

When verifying an `UncleProof` — the header it is about arrives as an argument, not in the proof
(`verify_uncle_proof(header, proof, merkle_root, target)`, `src/linear/src/block.rs:502`):

1. **PoW Hash Verification**: re-compute the RandomX PoW hash from the **header argument** using its
   `randomx_key`. There is no stored `pow_hash` to compare against; the computed hash is used in
   step 2 and then as the merkle leaf in step 3.

2. **Target Check**: verify the PoW hash meets the target (`hash_u32 <= target`). Higher target =
   easier mining. The target is the one at **the uncle's own height**, not the canonical block's
   (`verify_uncle_proof`'s parameter doc says so).

3. **Merkle Proof**: verify `blake3(header.to_mining_blob())` — not the header's serialization — is
   included at `proof.position` along `proof.merkle_path`, under the canonical block's
   `uncle_merkle_root`.

Steps 1 and 3 read differently from what this list said until 2026-10-12, and both differences come
from the same removal: the proof carries a path and a position, so the values its verification needs
are the ones the caller already holds.

```rust
pub fn verify_uncle_proof(
    header: &BlockHeader,
    proof: &UncleProof,
    merkle_root: &[u8; 32],
    target: BlockTarget,
) -> bool
```

It recomputes the PoW from `header` and checks the target, then checks `proof`'s Merkle
path and position against `merkle_root`. `UncleProof` holds only `merkle_path` and
`position` — there is no `pow_hash` field and no embedded `header` (`block.rs:297-302,
495-536`).

### Uncle Merkle Tree Construction

The canonical block's `uncle_merkle_root` is built from uncle proofs:

```rust
pub fn build_uncle_merkle(uncles: &[UncleBlock]) -> (blake3::Hash, Vec<UncleProof>) {
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

Do not quote a hand count here. `scripts/check-doc-canonical-counts.sh` derives the
population: **32 contracts, 31 with a `proof/` directory, 34 directories** (the extra two are
the `entropy` library and `test-harness`). The earlier "25 of 28" was stale and internally
inconsistent (`F-357`). The contract author should verify any new `init_contract` against the
survey below.

### Host Functions

WASM contracts can access these host functions:

| Function | Purpose |
|----------|---------|
| `db_init_`, `db_lookup_`, `db_get_`, `db_set_`, `db_del_` | State operations |
| `zkas_db_set_` | Store ZK circuit binaries |
| `merkle_add_`, `sparse_merkle_insert_batch_` | Merkle tree operations |
| `get_tx_hash_`, `get_call_index_`, `get_verifying_block_height_` | Context queries |

### Gas Metering

The WASM runtime uses `Metering` with `GAS_LIMIT = 400_000_000` points **and** a second
`BulkMemoryMetering` middleware that charges bulk-memory operators by length (`len >> 2`);
ops are no longer flat-priced (`vm_runtime.rs:118-131`, `bulk_metering.rs`; `OBL-C277`).

## Confirmation Model

DarkWow does not implement depth-based confirmation: no code reads a
block-depth threshold. Confirmation comes from two mechanisms, per
[consensus.md §Fork Choice Rule](consensus.md#fork-choice-rule):

1. **Finality anchors** — a block carrying a **verified Caribina (Arweave) anchor** SHALL
   NOT be displaced by reorg (the finality guard inside `detect_reorg`). The Monero anchor
   is not an enforced finality source: its fields are never written and
   `verify_monero_anchor` has no production caller.
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
2. The wallet scanner **does not re-verify ZK proofs**. `bin/dww/src/scan.rs` contains no
   verifier call; `verify_zkp` appears only in a `#[cfg(test)]` module
   (`prover_impl.rs:704,813`). The scanner decrypts notes and discovers capabilities, and
   it relies on the node's acceptance for proof validity.
3. A second verification point is therefore not present in the wallet today. Two-point
   verification would need the same verifying keys and the same acceptance decision on
   both sides (the Authenticated-Pool invariant, [mempool.md §1](../mempool.md)); the
   scanner-side half is a design target, not an implemented check.

## Limitations

1. **DarkLeaf tree**: Contract call hierarchy uses DarkLeaf at execution time (`src/linear/src/execution.rs`) — child call traversal is supported.
2. **Simplified state**: No overlay/rollback system means deterministic, reproducible state — a design feature, not a limitation.