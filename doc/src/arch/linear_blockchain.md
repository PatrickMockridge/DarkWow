# Linear Blockchain Architecture

The linear blockchain is a simplified WASM-based blockchain designed for testing and development. It uses Uncle Merkle consensus with a pin mechanism.

## Overview

The linear blockchain differs from the original DarkFi consensus in several key ways:

| Aspect | Original (Fork/Overlay) | Linear (Uncle Merkle) |
|--------|------------------------|----------------------|
| State management | Overlay + diffs + rollback | Plain sled |
| Fork resolution | Implicit competition | Explicit uncle reference |
| Mining risk | All-or-nothing | Bounded (uncle gets partial) |
| Verification | Heavy WASM + sled lookups | Merkle proof only |
| Determinism | Non-deterministic in time | Fully deterministic |
| Complexity | High | Low |

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
    pow_data: PowData,                 // Always PowData::DarkFi
    uncle_merkle_root: [u8; 32],
    total_reward: u64,
}
```

## RPC Endpoint

The `blockchain.get_block_linear` RPC endpoint returns wallet-compatible blocks:

```
Request:  {"jsonrpc": "2.0", "method": "blockchain.get_block_linear", "params": [height], "id": 1}
Response: {"jsonrpc": "2.0", "result": "base64encodedLinearBlockAdapter", "id": 1}
```

## ZK Verification

ZK proof verification in the linear blockchain differs from the original:

1. **darkfid validates all proofs** before including transactions in blocks
2. The wallet scanner **does not verify proofs** - it trusts that darkfid has validated them
3. Scanner's only job is **note decryption** to detect wallet-owned coins

This is a deliberate design choice that simplifies the wallet scanner significantly.

## Contract ID Conversion

Linear blockchain uses `blake3::Hash` for contract IDs, but wallet code expects `ContractId`:

```rust
fn hash_to_contract_id(hash: blake3::Hash) -> ContractId {
    ContractId::from_bytes(*hash.as_bytes()).unwrap()
}
```

## Limitations

1. **No DarkLeaf children_indexes**: Linear `ContractCall` lacks the `children_indexes` field, so DAO child call traversal is not possible
2. **No ZK verification in scanner**: Scanner trusts darkfid's validation
3. **Simplied state**: No overlay/rollback system means simpler but less powerful state management