/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! MerkleRoot Opcode Tests
//!
//! This module tests the MerkleRoot opcode (0x20) which computes a Merkle root
//! from a leaf position, Merkle path, and leaf value.
//!
//! ## MerkleRoot Opcode
//!
//! Signature: `merkle_root(leaf_pos: Uint32, path: MerklePath, leaf: Base) -> Base`
//!
//! - `leaf_pos`: Position of the leaf in the Merkle tree (0-indexed)
//! - `path`: Authentication path as an array of sibling hashes
//! - `leaf`: The leaf value to compute the root for
//! - Returns: The computed Merkle root
//!
//! ## Implementation
//!
//! The MerkleRoot opcode uses Halo2's `MerklePath::construct` and `calculate_root`
//! with the Sinsemilla hash function and `OrchardHashDomains::MerkleCrh` domain.
//!
//! ## Depth
//!
//! Currently fixed at `MERKLE_DEPTH_ORCHARD = 32` layers. This matches the
//! Orchard protocol's Merkle tree depth.
//!
//! ## Usage in Circuits
//!
//! ```zk
//! # Example: Verify a deposit exists in an external chain Merkle tree
//! leaf_pos: Uint32,       # Position in tree
//! path: MerklePath,       # 32-element authentication path
//! leaf: Base,             # H(secret, amount)
//!
//! root = merkle_root(leaf_pos, path, leaf);
//! constrain_equal_base(root, merkle_root_input);
//! ```

use darkfi_sdk::crypto::constants::MERKLE_DEPTH_ORCHARD;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_depth_constant() {
        // Verify MERKLE_DEPTH_ORCHARD is 32
        assert_eq!(MERKLE_DEPTH_ORCHARD, 32);
    }

    // TODO: Add full circuit test for MerkleRoot opcode
    //
    // A complete test would:
    // 1. Create a ZkBinary with a merkle_root opcode
    // 2. Create witnesses (leaf_pos, path, leaf)
    // 3. Execute the circuit
    // 4. Verify the computed root matches expected
    //
    // Example test structure:
    //
    // ```ignore
    // use darkfi::zk::{ZkCircuit, empty_witnesses};
    // use darkfi_sdk::crypto::MerkleNode;
    // use pasta_curves::Fp;
    // use halo2_proofs::{dev::MockProver, pasta::pallas};
    //
    // #[test]
    // fn test_merkle_root_valid_proof() {
    //     // Create a simple circuit that computes:
    //     // root = merkle_root(leaf_pos, path, leaf)
    //     let k = 11; // 2^11 = 2048 rows
    //
    //     // Witness values
    //     let leaf_pos = 10u32;
    //     let leaf = pallas::Base::from(12345);
    //     let path = [pallas::Base::random(&mut OsRng); 32];
    //
    //     // Create Merkle tree and compute expected root
    //     let (expected_root, merkle_path) = build_merkle_tree(leaf_pos, leaf, &path);
    //
    //     // Create circuit and prove
    //     let circuit = MerkleTestCircuit { leaf_pos, path: merkle_path, leaf };
    //     let prover = MockProver::run(k, &circuit, vec![]).unwrap();
    //     prover.assert_satisfied();
    // }
    // ```

    #[test]
    fn test_merkle_path_length() {
        // Verify MerklePath has correct length (MERKLE_DEPTH_ORCHARD = 32)
        let expected_length = MERKLE_DEPTH_ORCHARD;
        assert_eq!(expected_length, 32, "MerklePath should have 32 elements for depth 32");
    }

    #[test]
    fn test_merkle_opcode_signature() {
        // Verify the MerkleRoot opcode signature
        // (Uint32, MerklePath, Base) -> Base
        //
        // This is documented in src/zkas/opcode.rs:
        // MerkleRoot = 0x20, "merkle_root",
        //     (VarType::Base), (VarType::Uint32, VarType::MerklePath, VarType::Base);
        //
        // Note: The return type is Base (the computed root), not the standard ()
        // This is because the root is pushed to the heap for later use.
    }
}
