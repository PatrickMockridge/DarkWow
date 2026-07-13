# Linear ZKVM Architecture

> **Note:** This document describes a design vision. The types referenced here
> (`ContractStoreAccess`, `SimpleDbAccess`, `BlockchainAccess`) are not yet
> implemented as Rust source files. The actual WASM execution uses
> `dwow_core::runtime::vm_runtime::Runtime` via `bin/dwowd/src/execution.rs`.

How ZKVM functionality was replicated on the linear blockchain architecture.

## Context

The linear blockchain originally used a stateless ZK verification model without full WASM contract execution. This document explains how the complete ZKVM functionality (ZK proof verification + WASM contract execution) was replicated using trait-based adapters.

## ZKVM Components

| Component | File | Purpose |
|-----------|------|---------|
| `ZkVerifier` | `bin/dwowd/src/zk.rs` | Wrapper around `verify_zkp` for linear chain |
| `derive_vk()` | `src/validator/verification.rs:61` | Derives VerifyingKey from embedded zkbin_data |
| `Runtime::new()` | `src/runtime/vm_runtime.rs:379` | Creates WASM runtime for contract execution |
| WASM adapters | `src/linear_wasm_adapter.rs` | Trait implementations for linear storage |

## Verification Architecture

### Stateless Model (Linear)

Linear blockchain uses embedded `zkbin_data` for verification without WASM execution:

```rust
fn derive_vk(
    contract_id: &ContractId,
    zkas_ns: &str,
    zkbin_data: &[(ContractId, String, Vec<u8>, Vec<pallas::Base>)],
) -> Option<VerifyingKey> {
    // Find matching entry in embedded data
    let (_, _, zkbin_bytes, _) =
        zkbin_data.iter().find(|(cid, ns, _, _)| cid == contract_id && ns == zkas_ns)?;
    // Decode and build VK directly
    let zkbin = ZkBinary::decode(zkbin_bytes, false).ok()?;
    let circuit = ZkCircuit::new(empty_witnesses(&zkbin).ok()?, &zkbin);
    Some(VerifyingKey::build(zkbin.k, &circuit))
}
```

### Verification Flow

```
Block → zkbin_data [(ContractId, zkas_ns, zkbin_bytes, instances)] → derive_vk() → verify_zkp()
```

1. **Block inclusion**: dwowd validates all ZK proofs before adding transactions to blocks
2. **Embedded data**: Proof verification data is stored in `zkbin_data` field
3. **Stateless verification**: No WASM execution needed - VK derived from embedded bytes
4. **Trust model**: Wallet scanner trusts dwowd's validation; only performs note decryption

## WASM Runtime Architecture

### Trait-Based Storage Access

Linear uses trait abstractions for WASM contract storage:

```rust
pub trait ContractStoreAccess {
    fn lookup(&self, cid: &ContractId, tree_name: &str) -> Result<[u8; 32]>;
    fn init(&self, cid: &ContractId, tree_name: &str) -> Result<[u8; 32]>;
    fn insert_bincode(&self, cid: ContractId, bincode: &[u8]) -> Result<()>;
    fn get_bincode(&self, cid: &ContractId) -> Result<Vec<u8>>;
}

pub trait SimpleDbAccess {
    fn insert(&self, tree: &[u8], key: &[u8], value: &[u8]) -> Result<()>;
    fn get(&self, tree: &[u8], key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn remove(&self, tree: &[u8], key: &[u8]) -> Result<()>;
    fn contains_key(&self, tree: &[u8], key: &[u8]) -> Result<bool>;
}

pub trait BlockchainAccess {
    fn last_block_timestamp(&self) -> Result<Vec<u8>>;
    fn last_block_height(&self) -> Result<u32>;
    fn get_tx(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>>;
    fn get_tx_location(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>>;
    fn get_block_hash_by_height(&self, height: u32) -> Result<Option<Vec<u8>>>;
}
```

### Adapter Implementations

The `linear_wasm_adapter.rs` provides concrete implementations:

```rust
// Arc<LinearStore> implements ContractStoreAccess for WASM deployment
impl ContractStoreAccess for Arc<LinearStore> {
    fn lookup(&self, cid: &ContractId, tree_name: &str) -> Result<[u8; 32]> {
        let tree_key = format!("_contract_state_{}_{}", cid.to_hex(), tree_name);
        let hash = blake3::hash(tree_key.as_bytes());
        Ok(*hash.as_bytes())
    }
    // ... insert_bincode, get_bincode, etc.
}

// Arc<LinearStore> implements SimpleDbAccess for state operations
impl SimpleDbAccess for Arc<LinearStore> {
    fn insert(&self, tree: &[u8], key: &[u8], value: &[u8]) -> Result<()> {
        // tree is blake3 handle, key/value are state data
        self.contracts.insert(full_key.as_bytes(), value)?;
        Ok(())
    }
    // ... get, remove, contains_key
}

// Arc<CChainState> implements BlockchainAccess for state queries
impl BlockchainAccess for Arc<CChainState> {
    fn last_block_height(&self) -> Result<u32> {
        Ok(self.height as u32)
    }
    fn get_tx(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        // Query LinearStore for transaction
    }
    // ...
}
```

### Runtime Creation

The `Runtime::new()` uses these trait objects:

```rust
pub fn new(
    wasm_bytes: &[u8],
    contract_store: Arc<dyn ContractStoreAccess>,  // For deploy phase
    state_db: Arc<dyn SimpleDbAccess>,            // For exec/metadata/apply
    blockchain: Arc<dyn BlockchainAccess>,         // For state queries
    contract_id: ContractId,
    verifying_block_height: u32,
    block_target: u32,
    tx_hash: TransactionHash,
    call_idx: u8,
) -> Result<Self>
```

## ZkVerifier Integration

The `ZkVerifier` wraps the stateless `verify_zkp`:

```rust
pub struct ZkVerifier;

impl ZkVerifier {
    pub fn verify(
        &self,
        proof: &Proof,
        zkbin_bytes: &[u8],
        instances: &[pallas::Base],
    ) -> bool {
        verify_zkp(proof, zkbin_bytes, instances) == ZkVerifyResult::Ok
    }
}
```

Used in `CChainState`:

```rust
pub struct CChainState {
    pub store: Arc<LinearStore>,
    contract_store: Arc<dyn ContractStoreAccess>,
    state_db: Arc<dyn SimpleDbAccess>,
    pub consensus: PoWConsensus,
    pub zk_verifier: ZkVerifier,  // ZK verification
    height: AtomicU64,
}

impl CChainState {
    pub fn new(store: Arc<LinearStore>) -> Self {
        let zk_verifier = ZkVerifier;
        // Create adapters for WASM runtime
        let contract_store = crate::contract_store::LinearContractStore::new(store.clone());
        let state_db = crate::linear_simple_db::LinearSimpleDb::new(store.clone());
        // ...
    }
}
```

## Verification vs Execution

| Aspect | Verification | Execution (WASM Runtime) |
|--------|--------------|--------------------------|
| **Purpose** | Verify ZK proofs in blocks | Execute contract logic |
| **Trigger** | Block sync, transaction validation | Contract calls in blocks |
| **WASM** | Not used | `Runtime::new()` with wasmer |
| **State** | Read-only zkbin_data | Read/write via trait adapters |
| **Location** | `verification.rs` | `vm_runtime.rs` |

## Key Files

| File | Purpose |
|------|---------|
| `bin/dwowd/src/zk.rs` | ZkVerifier wrapper |
| `bin/dwowd/src/block_acceptor.rs` | CChainState with zk_verifier |
| `src/validator/verification.rs` | derive_vk(), verify_producer_transaction() |
| `src/runtime/vm_runtime.rs` | Runtime::new(), WASM execution |
| `src/linear_wasm_adapter.rs` | Trait implementations for linear storage |

## Design Trade-offs

### Linear's Approach (Stateless Verification)
- **Pros**: Fast sync, no WASM overhead, simple trust model
- **Cons**: Cannot extract ZK public inputs from WASM (must embed at block creation)

### Original DarkWow (Stateful WASM)
- **Pros**: Can extract public inputs at verification time
- **Cons**: Requires WASM runtime at every verification node

The linear design assumes block producers have already executed WASM to extract ZK public inputs, which are then embedded in blocks for lightweight verification.

## Related Documentation

- [ZK Verification](../zk/zk_verification.md) - Pure stateless proof verification
- [Linear Blockchain](./linear_blockchain.md) - Linear chain architecture overview
- [Wallet Scanning](../wallet_scanning.md) - Scanner trust model
- [Sync Module](../sync.md) - Block synchronization with verification