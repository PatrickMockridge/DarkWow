# Merge Mining FFI — Monero ↔ DarkWow Foreign Function Interface

> **Normative language (SHALL/MUST/SHOULD) per RFC 2119.**

This document specifies the formal contract between Monero (via P2Pool) and
DarkWow for merge mining. It follows the same FFI design principles as the
[Wallet FFI](../../dev/wallet-ffi.md): opaque handles, typed boundaries,
caller-provided buffers, NULL-safe, explicit error conventions. The merge
mining boundary is a foreign function interface between two independent type
systems — Monero's Keccak-based cryptography and DarkWow's Poseidon-based
capability model.

It SHALL be read together with:
- [Type System Specification](../type-system.md) — type distinction principle (§2), bytes round-trip forbidden (§2.2), error barbs (§4)
- [Consensus & Coinbase](../consensus-coinbase.md) — block production, coinbase ZK proof, emission schedule
- [Consensus](consensus.md) — 7-phase block validation, PoWRewardV1 nullifier claim
- [Uncle Merkle Consensus](uncle_merkle.md) — uncle inclusion, pin rewards
- [Linear Blockchain Architecture](linear_blockchain.md) — BlockHeader, PowSource enum
- [Wallet FFI](../../dev/wallet-ffi.md) — the proven FFI design template

## 1. Design Principles

The merge mining FFI SHALL follow the same type discipline as the wallet FFI,
grounded in the DarkWow type system.

### 1.1 Type Distinction

Per [type-system.md §2](../type-system.md), Monero types MUST be distinct
DarkWow newtypes. `MoneroBlockHash` IS NOT `[u8; 32]` just as `Nullifier` IS NOT
`[u8; 32]`. No Monero hash SHALL cross the FFI boundary without validation into
a typed wrapper.

### 1.2 Bytes Round-Trip Forbidden

Per [type-system.md §2.2](../type-system.md), no type SHALL be converted to
`[u8; 32]` and back across the module boundary. The merge mining FFI is a module
boundary between Monero and DarkWow — raw `seed_hash` bytes crossing MUST be
validated into a `RandomXKey` newtype. Raw `aux_hash` bytes MUST be validated
into a `JobId` newtype. Raw merkle proof elements MUST be validated into
`MoneroHash` newtypes.

### 1.3 Error Barbs

Per [type-system.md §4](../type-system.md), merge mining failures MUST be
distinct typed errors. The caller SHALL be able to distinguish "invalid seed
hash" from "merkle proof verification failed" from "stale template."

**Implemented.** `bin/dwowd/src/error.rs` defines `RpcError` variants with
unique numeric codes (-32324..-32343) for each parameter validation failure.
Submission outcomes use distinct status strings ("accepted" / "rejected" /
"stale") per P2Pool's expected response format (§6.2).

### 1.4 Opaque Handles

All Rust types cross the FFI boundary as opaque pointers. Callers MUST pair
every constructor with its corresponding destructor. The boundary SHALL follow
the seatuya pattern: `Box::new()` → `Box::into_raw()` → `Box::from_raw()`.

### 1.5 Determinism

The merge mining FFI SHALL be a deterministic function of its inputs. Given
identical Monero block data and DarkWow chain state, identical DarkWow blocks
SHALL be produced. The RandomX key derivation from seed_hash SHALL be
deterministic. The merkle proof verification SHALL be a pure function.

## 2. Type System — Monero ↔ DarkWow Mapping

### 2.1 Monero Types (Imported via P2Pool)

| Monero Type | Wire Format | DarkWow Newtype | Validation |
|------------|-------------|----------------|------------|
| `monero::Hash` | 32 bytes (Keccak-256) | `MoneroHash` | Length=32, non-zero |
| `seed_hash` | 32 bytes (RandomX VM key) | `RandomXKey` | Length=32, non-zero |
| `aux_hash` (job ID) | 32 bytes (blake3 of template) | `JobId` | Non-zero, registered in job table |
| `monero::Block` | Variable (consensus-encoded) | Raw bytes → `MoneroBlock` | Consensus-decode succeeds |
| `MerkleProof` | branch: Vec<32B>, path: u32 | `MoneroMerkleProof` | Branch length < 32, path non-zero |
| `coinbase_tx` | Variable (prefix + extra) | `MoneroCoinbasePrefix` | Extra field contains MM tag |
| `FixedByteArray` | ≤60 bytes | Validated at construction | Exact length match |
| `PowData` | Monero-specific struct | `PowSource::Monero(MoneroPowData)` | All fields non-default |

### 2.2 DarkWow Types (Constructed from Template)

| DarkWow Type | Constructed From | Validation |
|-------------|-----------------|------------|
| `BlockHeader` | Template + submitted PoW data | Height continuity, previous hash, merkle root |
| `Transaction` (coinbase) | `contract_calls: [PoWRewardV1]` | Coinbase MUST be first tx |
| `CoinbaseTransaction` | ZK proof + public inputs (9 elements) | Nullifier non-zero, proof non-empty |
| `Block` | Header + transactions + uncle merkle | `pow_source == Monero(MoneroPowData)` |
| `UncleBlock` | Competing blocks at same height | Depth ≤ MAX_UNCLE_DEPTH |

### 2.3 PowSource Bridging

The `PowSource` enum in `BlockHeader` carries the Monero-side proof:

```
PowSource::Monero(MoneroPowData {
    header: BlockHeader,              // Monero block header
    randomx_key: FixedByteArray,      // RandomX VM key
    transaction_count: u16,           // Monero tx count
    merkle_root: MoneroHash,          // Monero tx merkle root
    coinbase_merkle_proof: MerkleProof, // Proof coinbase is in tx tree
    coinbase_tx_hasher: KeccakState,  // Incomplete Keccak state
    coinbase_tx_extra: RawExtraField, // Coinbase extra field
    aux_chain_merkle_proof: MerkleProof, // Proof aux_hash is in aux chain
})
```

`PowSource::Monero` blocks SHALL skip native RandomX PoW verification and
instead verify the Monero merkle proof. This is the ONLY difference between
merge-mined and native-mined blocks — all other validation (Phase 0 structural
through Phase 6 SMT update) is identical.

## 3. P2Pool Bridge Protocol (mm_rpc)

The P2Pool merge mining protocol is specified at
<https://github.com/SChernykh/p2pool/blob/master/docs/MERGE_MINING.md>.
DarkWow implements three RPC methods as the aux chain server.

### 3.1 mm_get_chain_id — Chain Identity

```
Request:  {"method": "merge_mining_get_chain_id", "params": []}
Response: {"result": "<chain_id_hex>"}
```

The chain ID SHALL be the blake3 hash of the genesis block. It identifies
the DarkWow chain to P2Pool and SHALL be consistent across all nodes on
the same network.

### 3.2 mm_get_aux_block — Template Request

```
Request:  {"method": "merge_mining_get_aux_block", "params": {
    "address": "<wallet>",  // Ignored — dwowd mines to its declared key
    "aux_hash": "<hex>",    // Monero block hash being mined (dedup key)
    "height": <u64>,        // Monero block height (informational)
    "prev_id": "<hex>"      // Monero previous block hash (informational)
}}
Response: {"result": {
    "aux_blob": "",          // Always empty
    "aux_diff": <u64>,       // DarkWow difficulty target
    "aux_hash": "<hex>",     // Job ID = blake3(prev || height || merkle || ts)
    "job_id": "<hex>"        // Same as aux_hash
}}
```

The `aux_hash` input SHALL be stored in a job table keyed by the hex string.
Duplicate requests with the same `aux_hash` SHALL return an empty response
(p2pool re-polls with the same block hash). The `job_id` SHALL be
deterministic — recomputable from template contents.

### 3.3 mm_submit_solution — Solution Submission

```
Request:  {"method": "merge_mining_submit_solution", "params": [
    "<job_id>",        // hex-encoded blake3 job ID
    "<aux_blob>",      // hex-encoded aux blob (must be empty)
    "<blob>",          // hex-encoded serialized Monero block
    ["<h1>","<h2>"],   // merkle proof: array of 32-byte hex hashes
    <path>,            // merkle path bitmap (u32)
    "<seed_hash>"      // RandomX VM key (hex)
]}
Response: {"result": {"status": "accepted"}}
                       {"status": "rejected"}
                       {"status": "stale"}
                       {"status": "error", "message": "..."}
```

The submission SHALL be validated in the following order. Each step SHALL
fail fast — if any step fails, subsequent steps SHALL NOT execute.

1. **Structural validation** — JSON types, hex encoding, parameter presence
2. **Job registration** — `job_id` MUST be in the job table
3. **Duplicate detection** — `job_id` MUST NOT be in the submitted set
4. **Monero block deserialization** — `blob` MUST consensus-decode
5. **Merkle proof construction** — branch length MUST be < 32
6. **Merge mining tag extraction** — Monero coinbase tx_extra MUST contain
   exactly one `MergeMining(depth, hash)` tag
7. **Aux merkle proof verification** — `calculate_root(job_id) == extracted_root`
8. **MoneroPowData construction** — MUST succeed
9. **Coinbase merkle root validation** — `is_coinbase_valid_merkle_root()` MUST
   return true; coinbase MUST be leftmost leaf (`path_bitmap == 0`)
10. **Rate limiting** — `now - last_block_time >= min_block_interval`
11. **Block acceptance** — `accept_block()` through the standard path
12. **Broadcast** — `broadcast_block()` to P2P peers (HAZID H-C2 fix)

On acceptance, the block SHALL be broadcast to all P2P peers (HAZID H-C2,
fixed). On rejection, a "rejected" status SHALL be returned. On rate-limit
violation, a "stale" status SHALL be returned.

## 4. Template Lifecycle

### 4.1 Creation

A template SHALL be created fresh on each `mm_get_aux_block` call. The template
includes:
- ZK coinbase proof (Mint_V1 circuit, PoWRewardV1 call)
- Selected mempool transactions (fee-descending, gas-capped)
- Uncle blocks from competing block storage
- Complete Merkle roots (transactions, commitment, nullifier)

### 4.2 Caching

The template SHALL be cached in `MiningState.current_linear_template` for
block reconstruction during submission. The template cache SHALL be
invalidated and regenerated when:
- A new block is accepted (height changes)
- The mempool changes significantly (tx selected or evicted)
- A new competing block arrives (uncle merkle root changes)

### 4.3 Versioning

The job ID SHALL be a deterministic function of template contents:
`job_id = blake3(previous_hash || height || merkle_root || timestamp)`.
A submission with a job ID that does not match the cached template SHALL
be rejected. This prevents replay attacks and stale submissions.

### 4.4 Job Table Eviction

The job table SHALL use FIFO eviction, not clear-all. When the table reaches
capacity (MAX_MM_JOBS = 100), the oldest entry SHALL be evicted, not all
entries (HAZID H-M11 fix). The submitted set SHALL use the same FIFO policy
with MAX_MM_SUBMITTED = 1000.

### 4.5 Competing Block Handling

Competing blocks SHALL NOT be consumed before template generation. The
validate-then-mutate pattern SHALL apply: `generate_linear_block_template()`
(which includes ZK coinbase proof generation) SHALL succeed before
`take_competing_blocks()` is called (HAZID findings #4, #5 fix).

## 5. Block Reconstruction

### 5.1 Monero → DarkWow Translation

From the submitted Monero block data, the DarkWow block SHALL be reconstructed
as follows:

1. **Block header**: From the cached template, with `pow_source` set to
   `PowSource::Monero(MoneroPowData)` and `randomx_key` from the submitted
   `seed_hash` (validated as `RandomXKey`).

2. **Coinbase transaction**: From the template's pre-computed ZK coinbase
   (PoWRewardV1 contract call with Mint_V1 proof), assembled as
   `transactions[0]`.

3. **Mempool transactions**: From the template's selected mempool transactions,
   appended after the coinbase.

4. **Merkle root**: Recomputed over all transactions using blake3 binary
   Merkle tree (NOT Monero's Keccak tree hash — the two chains use
   different hash functions).

5. **Uncle merkle root**: From the template's pre-computed uncle merkle tree.

### 5.2 PoW Verification

Merge-mined blocks SHALL skip native RandomX PoW verification (Stage 1
in `accept_block()`). The Monero PoW is implicitly verified by:

1. The Monero merkle proof — the aux_hash (job ID) is proven to be a leaf
   in the Monero coinbase's merkle tree.
2. The Monero block header — the merkle root embedded in the coinbase tx_extra
   is proven to be the root of the Monero transaction merkle tree.
3. The Monero consensus — the Monero block's own PoW (RandomX over the Monero
   block header) is verified by the Monero network.

These three proofs together SHALL provide equivalent security to native
RandomX verification: a valid merge-mined block proves that a Monero miner
expended RandomX work at the Monero network difficulty.

### 5.3 Block Acceptance

After reconstruction, the block SHALL enter the standard acceptance path
(`accept_block()`), which performs:
- Phase 0: Structural validation (coinbase at transactions[0])
- Proof of token balance (mass balance equation)
- Stage 2 PoW: Target matches consensus (declared target == expected)
- L2 witness verification (ZK proofs + signatures for non-coinbase txs)
- WASM execution (pow_reward_v1, cumulative supply chain)
- Overlay aggregation + atomic commit

The block SHALL then be broadcast to P2P peers via `broadcast_block()`.

## 6. Error Model

### 6.1 Error Codes

Merge mining errors SHALL be distinct typed errors per [type-system.md §4](../type-system.md).
All error codes are implemented in `bin/dwowd/src/error.rs` (enum `RpcError`).

| Error Barb | Error Code | Condition | Status |
|-----------|-----------|-----------|--------|
| `↓bad-input` | -32324..-32341 | Parameter validation (missing, wrong type, invalid hex, wrong length) | Implemented |
| `↓bad-proof` | -32342..-32343 | Merkle proof construction/verification failed | Implemented |
| `↓bad-nullifier` | — | Duplicate job submission | Status response "rejected" |
| `↓double-spend` | — | Job already submitted (same job_id) | Status response "rejected" |
| `↓rate-limit` | — | Submission within min_block_interval | Status response "stale" |
| `↓bad-proof` | — | accept_block() rejected the reconstructed block | Status response "rejected" |
| `↓db-fail` | -32350 | Node not synced | Implemented |
| `↓db-fail` | -32350 | Node not synced |

### 6.2 Response Format

Two response patterns SHALL be used:

1. **JSON-RPC error** (`server_error(code, id, msg)`): For parameter validation
   failures and internal errors before submission processing.

2. **Status response** (`{"status": "accepted"|"rejected"|"stale"}`):
   For submission outcomes and post-validation failures. This SHALL match
   P2Pool's expected response format.

### 6.3 Last Error Retrieval

The merge mining handle SHALL store the last error as a human-readable string,
retrievable via `dwow_mm_last_error(handle, buf, len)`. The error SHALL be
cleared after retrieval.

## 7. Security Model

### 7.1 What P2Pool Can Do

P2Pool acts as a bridge between Monero and DarkWow. It:
- Requests DarkWow block templates to embed in Monero coinbase tx_extra
- Submits Monero blocks containing valid RandomX PoW with embedded aux_hash
- Provides merkle proofs that the aux_hash is in the Monero coinbase tree

### 7.2 What P2Pool Cannot Do

P2Pool cannot:
- Produce a valid DarkWow block without valid Monero PoW (merkle proof binds to Monero coinbase)
- Double-submit the same job (job table dedup)
- Replay a stale job (job ID verification against cached template)
- Modify DarkWow consensus rules (accept_block runs identically for merge-mined and native blocks)
- Access DarkWow private keys (coinbase ZK proof is pre-computed by dwowd)

### 7.3 What DarkWow Guarantees

DarkWow guarantees:
- Every merge-mined block carries a valid ZK coinbase proof (Mint_V1 circuit)
- The PoWRewardV1 nullifier claim is verified identically for merge-mined and native blocks
- Cumulative supply chain integrity is maintained (S_H = S_{H-1} + C_H)
- Uncle merkle consensus applies equally to merge-mined blocks
- After acceptance, the merge-mined block is broadcast to all P2P peers

## 8. Architecture

```
┌──────────┐     stratum      ┌──────────┐     mm_rpc (JSON-RPC)     ┌──────────┐
│  xmrig   │◄───────────────►│  p2pool  │◄────────────────────────►│  dwowd   │
│          │   mining jobs    │          │   get_chain_id             │          │
│  Monero  │   share submit   │  Bridge  │   get_aux_block            │  DarkWow │
│  PoW     │                  │          │   submit_solution          │  Chain   │
└──────────┘                  └────┬─────┘                           └────┬─────┘
                                   │                                     │
                                   │  Monero RPC                    ┌────▼─────┐
                                   └───────────────────────────────►│ monerod  │
                                      get_block, get_height         │  Node    │
                                                                    └──────────┘

Monero Type System                  Bridge (P2Pool)                 DarkWow Type System
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
monero::Hash (Keccak-256) ──────►  hex-encoded  ──────►  MoneroHash newtype
seed_hash (32 bytes)      ──────►  hex-encoded  ──────►  RandomXKey newtype
monero::Block              ──────►  hex-encoded  ──────►  MoneroPowData
MerkleProof (branch+path)  ──────►  JSON array   ──────►  MoneroMerkleProof
                             ▲                             │
                             │    FFI Boundary              │
                             │    All types validated       │
                             │    before crossing           │
                             └─────────────────────────────┘
```

## 9. API Reference

### 9.1 Lifecycle

| Function | Signature | Purpose |
|----------|-----------|---------|
| `dwow_mm_chain_id` | `int32_t(handle, out_buf, buf_len) → bytes` | Get chain ID (blake3 of genesis) |
| `dwow_mm_version` | `const char*()` | Library version string |

### 9.2 Template

| Function | Signature | Purpose |
|----------|-----------|---------|
| `dwow_mm_get_aux_block` | `int32_t(handle, aux_hash_hex, out_buf, buf_len) → template JSON` | Request mining template |
| `dwow_mm_aux_hash` | `int32_t(handle, out_buf_32) → 32` | Get aux_hash/job_id (32 bytes) |

### 9.3 Submission

| Function | Signature | Purpose |
|----------|-----------|---------|
| `dwow_mm_submit_solution` | `int32_t(handle, job_id_hex, blob_hex, merkle_proof_json, path, seed_hash_hex, out_status_buf, buf_len) → status len` | Submit solution |
| `dwow_mm_last_error` | `int32_t(handle, out_buf, buf_len) → error len` | Get last error message |

### 9.4 Recovery

| Function | Signature | Purpose |
|----------|-----------|---------|
| `dwow_mm_job_count` | `int32_t(handle) → N` | Count of pending jobs |
| `dwow_mm_submitted_count` | `int32_t(handle) → N` | Count of submitted jobs |

## 10. Verification

### 10.1 Spec Consistency

```bash
grep -c "SHALL\|MUST" doc/src/arch/consensus/merge-mining-ffi.md
```

### 10.2 Pipeline Test

```bash
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode merge --with-wallet 2 --fresh
```

### 10.3 Cross-Reference

- [Type System Specification](../type-system.md) — §2, §2.2, §4, §8.1, §8.2
- [Consensus & Coinbase](../consensus-coinbase.md) — §1, §2, §4
- [Consensus](consensus.md) — §PoWRewardV1 Nullifier Claim
- [Uncle Merkle Consensus](uncle_merkle.md) — §Uncle Generation, §Reward Distribution
- [Linear Blockchain Architecture](linear_blockchain.md) — §BlockHeader, §PowSource
- [Wallet FFI](../../dev/wallet-ffi.md) — Design template and patterns
- [Merge Mining Architecture](../merge-mining.md) — Docker setup, operator guide
- [Monero Merge Mining](../monero-merge-mining.md) — Protocol details, 228-byte blob
