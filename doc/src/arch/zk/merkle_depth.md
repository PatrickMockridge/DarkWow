# MerkleRoot Opcode: Depth Limitation

## Current Implementation

The `MerkleRoot` opcode (0x20) uses a **fixed depth of 32**:

```rust
let merkle_path: Value<[Fp; MERKLE_DEPTH_ORCHARD]> = ...
// where MERKLE_DEPTH_ORCHARD = 32
```

This matches the Zcash Orchard protocol's Merkle tree depth.

## The Problem

Different blockchain ecosystems use different Merkle tree depths:

| Chain | Merkle Tree Depth |
|-------|-------------------|
| Ethereum (Verkle) | 256 |
| Bitcoin | Variable (often 1-64) |
| Zcash Orchard | 32 |
| Custom applications | Variable |

The current implementation **cannot** verify Merkle proofs from trees with depth ≠ 32.

## Why Depth Matters

In Halo2, the circuit's row count and gate configuration must be fixed at synthesis time. The `MerklePath::construct` and `calculate_root` functions from `halo2_gadgets::sinsemilla::merkle` are parameterized by depth at the type level.

Changing the depth requires:
1. A new `MerkleChip` configured for the desired depth
2. Different advice columns and permutation columns
3. A new opcode to use the new chip

## Workarounds

### 1. Truncate or Pad the Path

If the actual tree depth is **less than 32**:
- Provide only the first `actual_depth` elements of the path
- Set remaining elements to the identity element (usually 0)
- The circuit will compute a "partial" root that must be manually compared

If the actual tree depth is **greater than 32**:
- The proof cannot be verified with the current opcode
- Need either: (a) truncate to 32 levels if the top 32 levels are the relevant part, or (b) implement a new opcode

### 2. Custom Opcode for Specific Depths

For a specific non-32 depth (e.g., 4 for a simple app), you could:

1. Create a new `MerkleChip` configured for depth 4
2. Add a new opcode `MerkleRootDepth4` that uses it
3. Use this opcode for circuits that only need depth-4 trees

Example structure:
```rust
// In opcode.rs
MerkleRootDepth4 = 0x22, "merkle_root_d4",
    (VarType::Base), (VarType::Uint32, VarType::MerklePathDepth4, VarType::Base);

// Where MerklePathDepth4 is a 4-element array type
```

### 3. Use SparseMerkleTree Instead

For applications that don't need the full Orchard Merkle hash, the `SparseMerkleRoot` opcode (0x21) uses a Poseidon-based SMT with depth 3 (`SMT_FP_DEPTH = 3`). This is suitable for:
- Small trees (up to 8 leaves)
- Applications already using Poseidon hashing

## Recommendation for Bridge Contract

For the bridge contract, the external chain (e.g., Ethereum) likely uses a different Merkle tree structure entirely (e.g., Keccak-based for Ethereum's state trie, or a different depth).

**Current approach**: The `deposit_v1.zk` circuit accepts `merkle_root_input` as a public parameter and verifies the proof against it. This is correct for the proof structure, but the actual bridge integration must ensure:

1. The `merkle_root_input` is trusted (from a light client or validator)
2. The `leaf_pos` and `path` are correctly obtained from the external chain's Merkle structure

**For production**: A light client integration should verify the `external_block_hash` corresponds to a valid block, and the Merkle proof should be for the specific tree structure used by that chain.

## Implementing Variable Depth Merkle

To properly support variable depths, one would need to:

1. **Generic Merkle Chip**: Create a `MerkleChip<const D: usize>` parameterized by depth
2. **Runtime Depth Checking**: Use Halo2's dynamic selectors to enable/disable gates based on actual depth used
3. **Precompiled Circuits**: Generate circuits for common depths (4, 16, 32, 256) at compile time

This is significant engineering work and would increase proving key sizes.

---

**See also**:
- [MerkleRoot opcode implementation](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/zk/vm.rs) - lines 1012-1050
- [deposit_v1.zk circuit](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/contract/bridge/proof/deposit_v1.zk) - Example usage with depth-32 tree
- [halo2_gadgets::sinsemilla::merkle](https://docs.rs/halo2_gadgets) - Underlying Halo2 implementation
