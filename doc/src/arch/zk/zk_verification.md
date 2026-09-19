# ZK Verification

> **Note:** The core ZK verification infrastructure (`verify_zkp`, ZkBinary format, ZKCircuit structure) is inherited from upstream DarkWow and tracks upstream. The chain integration (`verify_single_tx`, witness reconciliation) and the linear block-accept flow are DarkWow-specific.

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

## Usage in the chain

The chain-side entry point is `verify_single_tx` in
`src/linear/src/zk_verifier.rs`, wired at both mempool admission and block
accept. It:

1. decodes the chain tx's `witness` and reconciles it against the tx's
   `contract_calls` (`decode_and_reconcile`) — the witness is hash-excluded, so
   a divergent witness is hard-rejected;
2. loads each called circuit's `zkbin` (`load_zkbin`) from the contract's
   registered zkas namespace;
3. calls `verify_zkp(proof, zkbin_bytes, instances)` per proof.

Proofs ride in the transaction, not in a block-level side vector: the witness is
`dwow_serial(dwow_core::tx::Transaction)`, whose `proofs` field is one group per
contract call, each group holding that call's proofs.

## Non-ZK calls

Three native-token functions carry **no** proof, and `verify_single_tx` exempts
them by selector:

| Selector | Function | Why there is no proof |
|---|---|---|
| `0x05` | `PoWRewardV1` (coinbase) | The reward value is public (`expected_reward(H)`, Σ pin). The WASM entrypoint checks it in the clear. |
| `0x06` | `FeeCollectV1` | Plaintext redistribution of an already-public fee pot. |
| `0x07` | `UncleMintV1` | The uncle pin is derived from depth by `check_uncles`, not proven. |

The coinbase is additionally exempt at the block level: its soundness is
transparent WASM re-execution of `PoWRewardV1` (see
[consensus-coinbase.md](../consensus-coinbase.md) §2.5), not a proof.

## Example: FeeV3 Verification

FeeV3 (`0x08`) is an ordinary ZK-gated call — the `Fee_V3` mass-balance circuit
proves `input = output + fee` while the fee amount itself stays plaintext in
`FeeParamsV3`.

1. **Proof generation** (at transaction build time), via the contract client:
   ```rust
   let result = FeeV3CallBuilder { .. }.build()?;
   // result.params: FeeParamsV3, result.proofs: Vec<Proof>
   ```

2. **Carried in the transaction witness** — `result.proofs` goes into the core
   tx alongside the call.

3. **Verify at admission and at block accept**:
   ```rust
   verify_single_tx(chain_tx)?;   // → verify_zkp(proof, zkbin_bytes, instances)
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

- [Sync Module](../sync.md) - How verification is used in block sync
- [ZKAS](../../zkas/zkas.md) - ZK circuit format
- [ZKVM](../../zkas/zkvm.md) - Virtual machine for ZK execution