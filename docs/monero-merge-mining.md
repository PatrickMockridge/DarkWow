# Monero Merge Mining

DarkWow supports merge mining with Monero via the [p2pool](https://github.com/SChernykh/p2pool)
protocol. A miner running `xmrig` hashes the **Monero block** (RandomX). When a share meets
DarkWow's difficulty target, p2pool submits the solution to dwowd's `mm_rpc` JSON-RPC endpoint.

## Architecture

```
xmrig --[stratum]--> p2pool --[merge_mining_submit_solution]--> dwowd (mm_rpc HTTP JSON-RPC)
                  /    \--[monerod RPC]--> monerod (testnet, synced)
```

There is **no adaptor**. p2pool connects directly to dwowd's merge mining RPC server on a
dedicated port (default: `31348` on darkwow-testnet, `28348` on darkwow-devnet).

## Protocol

dwowd implements the three p2pool merge mining RPC methods defined in
[MERGE_MINING.MD](https://github.com/SChernykh/p2pool/blob/master/docs/MERGE_MINING.MD):

| Method | Purpose |
|--------|---------|
| `merge_mining_get_chain_id` | p2pool discovers the aux chain identity |
| `merge_mining_get_aux_block` | p2pool requests aux data for a Monero block template |
| `merge_mining_submit_solution` | p2pool submits a solution with cryptographic proofs |

The `chain_id` is computed as `blake3(genesis_hash || "testnet" || 0u32.to_le_bytes())`.

## Mining Blob Format

The mining blob is **228 bytes**:

| Bytes | Field |
|-------|-------|
| 0..227 | DarkWow block header (serialized) |
| 227 | `pow_source` discriminator: `0x00` = Native, `0x01` = Monero |

The `pow_source` discriminator tells the blockchain which PoW verification path to use.
For merge-mined blocks, the DarkWow header is NOT hashed by xmrig — xmrig hashes the
Monero block. The PoW is verified cryptographically via the three receipts below.

## PowSource::Monero(MoneroPowData)

When a merge-mined block is accepted, its `pow_source` field is set to
`PowSource::Monero(MoneroPowData)`. This struct contains:

```rust
pub struct MoneroPowData {
    /// The Monero block header (for RandomX verification by monerod)
    pub block_header: monero::BlockHeader,
    /// RandomX seed hash — the Monero seed hash used as the RandomX key
    pub randomx_key: [u8; 32],
    /// Number of transactions in the Monero block
    pub transaction_count: u64,
    /// Monero block transaction merkle root (tree_hash)
    pub merkle_root: monero::Hash,
    /// Kecak-based merkle proof: coinbase tx is in the Monero block tx tree
    pub coinbase_merkle_proof: MerkleProof,
    /// Partially-hashed coinbase prefix state (for Kecak verification)
    pub coinbase_tx_hasher: [u8; 200],
    /// The coinbase transaction's tx_extra field (contains merge mining tag)
    pub coinbase_tx_extra: Vec<u8>,
    /// Aux chain merkle proof: aux_hash is a leaf in the merge mining tag tree
    pub aux_chain_merkle_proof: MerkleProof,
}
```

## Three Cryptographic Receipts

Each `mm_submit_solution` must pass three independent verifications:

### Receipt 1: Merge Mining Tag Extraction

`extract_aux_merkle_root_from_block(monero_block)` parses the Monero coinbase
transaction's `tx_extra` field and extracts a `SubField::MergeMining` entry.
This entry contains an **aux merkle root** — the root of a merkle tree of all
aux chain hashes being merge-mined.

- **Proves:** The Monero miner is aware of DarkWow and included a merge mining tag
- **Code:** `src/linear/src/monero/mod.rs` → `extract_aux_merkle_root()`

### Receipt 2: Aux Merkle Proof

`MerkleProof::calculate_root(&aux_hash)` takes the solution's `aux_hash` (from p2pool)
and a merkle proof (also from p2pool) and computes the expected merkle root. If this
matches the extracted aux merkle root from Receipt 1, the proof is valid.

- **Proves:** The submitted `aux_hash` is a legitimate leaf in the merge mining tag's
  merkle tree — the miner solved a real Monero block, not a fabricated one
- **Code:** `src/linear/src/monero/merkle_proof.rs` → `calculate_root()`

### Receipt 3: Coinbase Merkle Proof

`is_coinbase_valid_merkle_root()` verifies that the coinbase transaction is part of the
Monero block's transaction merkle tree. It uses the Keccak-based `coinbase_tx_hasher`
(partial hash state from prefix hashing) to compute the coinbase hash, then walks the
merkle proof to verify it matches the block's `merkle_root`.

- **Proves:** The MoneroPowData's coinbase transaction is the actual coinbase of the
  Monero block, not a fake one
- **Code:** `src/linear/src/monero/mod.rs` → `is_coinbase_valid_merkle_root()`

## PoW Verification Path

For `PowSource::Monero` blocks, native PoW verification is **skipped**:

1. `Block::hash()` hashes the DarkWow header using DarkWow's RandomX key
2. xmrig hashes the **Monero block** using Monero's RandomX key
3. These are different inputs with different keys — the native check would always fail

Instead, the PoW is **cryptographically verified** in `mm_submit_solution`:

```
mm_submit_solution
  → extract_aux_merkle_root_from_block()     # Receipt 1
  → MerkleProof::calculate_root()            # Receipt 2
  → is_coinbase_valid_merkle_root()          # Receipt 3
  → Construct Block with PowSource::Monero(MoneroPowData)
  → apply_block() → skip native PoW for Monero source
```

The Monero block's PoW is verified by monerod — dwowd only needs to prove the
DarkWow data was included in that already-verified Monero block.

## Block Construction (mm_submit_solution)

When a valid solution arrives:

1. `MoneroPowData::new(block, seed_hash, merkle_proof)` computes the full proof container
2. A `BlockHeader` is assembled from the current template (height, target, timestamp, etc.)
3. A coinbase transaction is created with the block reward
4. The merkle root is recomputed from all transactions (template txs + coinbase)
5. The block is submitted via `apply_block()`

The merkle root **must be recomputed** because the template's merkle root only covers
mempool transactions — the coinbase is added at submission time.

## End-to-End Flow

```
1. monerod mines/generates a new Monero block
2. p2pool polls dwowd for aux chain data (merge_mining_get_aux_block)
3. dwowd returns: aux_hash (unique job ID), empty aux_blob
4. p2pool creates a stratum job for xmrig (includes aux_hash)
5. xmrig hashes the Monero block header (RandomX)
6. When xmrig finds a share meeting difficulty:
   a. p2pool calls merge_mining_submit_solution with:
      - aux_hash (job ID from step 3)
      - Monero block blob (full serialized block)
      - Merkle proof (aux_hash → merge mining tag root)
      - Seed hash (RandomX key)
   b. dwowd deserializes the Monero block
   c. dwowd verifies all three cryptographic receipts
   d. dwowd constructs and applies the DarkWow block
   e. dwowd generates a new template for the next round
```

## Testing

### Prerequisites

- Monero testnet daemon (`monerod`) with synced blockchain data
- p2pool binary (v4.14+)
- xmrig (v6.22.2+)
- Rust toolchain

### One-time Setup

```bash
# Sync Monero testnet (takes 8-12 hours, do once)
monerod --testnet --data-dir ~/.cache/dwow_merge_testnet_monero \
  --fast-block-sync=1 --hide-my-port \
  --add-peer 125.229.105.12:28081
```

### Running the E2E Test

```bash
# Build (10 threads max)
RAYON_NUM_THREADS=10 cargo build -p dwowd --release

# Run
RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 \
  bash contrib/docker/darkwow-testnet/test_merge_mining_p2pool.sh
```

### Thread Budget

| Service | Threads | Notes |
|---------|---------|-------|
| xmrig | 1 (`-t 1`) | Single-threaded mining at difficulty 1000 |
| monerod | 4 (`--max-concurrency 4`) | Offline, fixed-difficulty |
| p2pool | ~1-2 | Default |
| dwowd | ~1-3 | Node + mm_rpc |
| cargo build | 10 (`RAYON_NUM_THREADS=10`) | |

**Total: ~12 threads max** — safe for any machine.

### Debugging

When the test fails, check in order:

1. **Stale ports:** `ss -tlnp | grep -E "28081|31345|31348|3333|37888"` — should be empty before start
2. **mm_rpc endpoint:** `curl -X POST http://127.0.0.1:31348 -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"merge_mining_get_chain_id","params":[],"id":1}'` — must return chain_id
3. **dwowd log:** `grep "RPC-MM" /tmp/dwow_merge_test/dwowd.log` — check submissions received and proofs verified
4. **Block acceptance:** `grep "Merge-mined block.*accepted" /tmp/dwow_merge_test/dwowd.log`
5. **PoW verification:** `grep "failed PoW\|MerkleRootMismatch" /tmp/dwow_merge_test/dwowd.log` — should be empty
6. **p2pool connectivity:** `grep "submit_aux_block" /tmp/dwow_merge_test/p2pool.log`
7. **xmrig hashrate:** `grep "speed" /tmp/dwow_merge_test/xmrig.log | tail -5`

## Key Source Files

| File | Purpose |
|------|---------|
| `bin/dwowd/src/rpc/mm_rpc.rs` | Merge mining RPC handler (p2pool protocol) |
| `src/linear/src/monero/mod.rs` | MoneroPowData, extract_aux_merkle_root, is_coinbase_valid_merkle_root |
| `src/linear/src/monero/merkle_proof.rs` | MerkleProof (Kecak-based, Monero-compatible) |
| `src/linear/src/block.rs` | BlockHeader, PowSource, verify_merkle_root |
| `src/linear/src/blockchain.rs` | apply_block — skips native PoW for PowSource::Monero |
| `bin/dwowd/src/blockchain.rs` | apply_block_with_uncles — same skip for full node |
| `bin/dwowd/src/registry/model.rs` | LinearBlockTemplate, generate_linear_block_template |
| `contrib/docker/darkwow-testnet/test_merge_mining_p2pool.sh` | E2E test script |
