# ZK Verification

> **Note:** The core ZK verification infrastructure (`verify_zkp`, ZkBinary format, ZKCircuit structure) is inherited from upstream DarkFi and tracks upstream. The PoW reward verification integration and linear blockchain sync flow are DarkWow-specific.

Pure, stateless ZK proof verification for DarkWow.

## Overview

The ZK verifier module provides deterministic proof verification without any side effects (no sled, no WASM, no global state).

## verify_zkp Function

```rust
pub fn verify_zkp(
    proof: &Proof,
    zkbin_bytes: &[u8],
    instances: &[pallas::Base],
) -> ZkVerifyResult
```

### Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `proof` | `&Proof` | The ZK proof to verify |
| `zkbin_bytes` | `&[u8]` | ZkBinary circuit bytes |
| `instances` | `&[pallas::Base]` | Public inputs |

### Returns

```rust
pub enum ZkVerifyResult {
    Ok,           // Proof is valid
    InvalidProof, // Proof verification failed
    InvalidVk,    // Could not derive VK from circuit bytes
}
```

## How It Works

1. **Decode**: `ZkBinary::decode(zkbin_bytes, false)` parses the circuit
2. **Circuit**: `ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin)` creates circuit with empty witnesses
3. **Derive VK**: `VerifyingKey::build(zkbin.k, &circuit)` derives VK from circuit
4. **Verify**: `proof.verify(&vk, instances)` verifies the proof

## Design Principles

1. **Stateless**: No sled, no WASM, no side effects
2. **Deterministic**: Same inputs → same output
3. **Separated**: Independent from sync, consensus, and block production

## Usage in Sync

During block sync, `verify_zkp` is called from `sync::verify_block`:

```rust
pub async fn verify_block(
    block: &BlockInfo,
    previous: &BlockInfo,
    zkbin_data: &[ZkBinEntry],
) -> Result<()> {
    // Build lookup: (contract_id, zkas_ns) -> (zkbin_bytes, instances)
    let zkbin_index = build_index(zkbin_data);

    for tx in &block.txs {
        for (call_idx, call) in tx.calls.iter().enumerate() {
            for proof in &tx.proofs[call_idx] {
                let (zkbin_bytes, instances) = zkbin_index.get(...)?;
                match verify_zkp(proof, zkbin_bytes, instances) {
                    ZkVerifyResult::Ok => {}
                    _ => return Err(Error::ZkvmVerificationFailed),
                }
            }
        }
    }
    Ok(())
}
```

## ZkBinEntry Format

For sync verification, proof data is carried in `ZkBinEntry`:

```rust
pub type ZkBinEntry = (ContractId, String, Vec<u8>, Vec<pallas::Base>);
//                       contract_id,  zkas_ns,  zkbin_bytes,  instances
```

- `contract_id`: Identifies the contract (e.g., `NATIVE_TOKEN_CONTRACT_ID`)
- `zkas_ns`: Circuit namespace (e.g., "Mint_V1", "Burn_V1")
- `zkbin_bytes`: Compiled ZkBinary circuit
- `instances`: Public inputs for this specific proof

## Example: PoWReward Verification

When a block contains a PoW reward transaction:

1. **Proof generation** (at block creation):
   ```rust
   let (proof, public_inputs) = create_transfer_mint_proof(
       &mint_zkbin,
       &mint_pk,
       &output,
       value_blind,
       token_blind,
       spend_hook,
       user_data,
       coin_blind,
   )?;
   ```

2. **Store in block**:
   ```rust
   block.zkbin_data = vec![(
       *NATIVE_TOKEN_CONTRACT_ID,
       "Mint_V1".to_string(),
       zkbin_bytes,  // from include_bytes!
       public_inputs.to_vec(),
   )];
   ```

3. **Verify at sync**:
   ```rust
   verify_zkp(&proof, &zkbin_bytes, &instances)
   ```

## File Location

```
src/zk/verifier.rs
```

Exported from:
```
src/zk/mod.rs
```

## Related

- [Sync Module](./sync.md) - How verification is used in block sync
- [ZKAS](./zkas/zkas.md) - ZK circuit format
- [ZKVM](./zkas/zkvm.md) - Virtual machine for ZK execution