//! Two-Level Merkle Tree Anchoring — shared infrastructure for L1 o-cap contracts.
//!
//! # ρ-Calculus Foundation
//!
//! The two-level Merkle tree architecture IS nested restriction in the ρ-calculus:
//!
//! ```text
//! ν(block_tree).(
//!   coinbase
//!   | ν(box_tree).(Put | Take)
//!   | ν(purse_tree).(Deposit | Withdraw | Balance)
//!   | ...
//! )
//! ```
//!
//! The nullifier is the extruded name — created inside `ν(contract_tree)`,
//! emitted through the restriction, and visible in `ν(block_tree)`. It links
//! the two levels.
//!
//! # Verification Path
//!
//! ```text
//! nullifier
//!   → contract-local Merkle proof (proves object existed in contract tree)
//!   → contract tree root
//!   → block-level Merkle proof (proves (nullifier, contract_root) is in block tree)
//!   → block header (commits to block tree root)
//! ```
//!
//! Every L1 o-cap contract (Box, Purse, PromissoryNote, and all future
//! contracts) uses this module. No per-contract anchoring code is permitted.
//!
//! Reference: contract-wasm-type-system.md Part C §C.3.7.

use crate::crypto::{ContractId, MerkleNode, Nullifier};
use crate::error::ContractError;
use crate::pasta::pallas;

/// Merkle authentication path (32 Sinsemilla-hashed nodes).
pub type MerklePath = [MerkleNode; 32];

/// Computes the anchored leaf value: `poseidon_hash(nullifier || contract_id || contract_root)`.
///
/// This is the value stored in the block-level Merkle tree, keyed by nullifier.
/// The Poseidon hash commits to the nullifier, contract identity, and contract tree
/// root, making each anchor entry uniquely bound to its nullifier.
///
/// ρ-calculus: the nullifier IS the extruded name linking ν(contract_tree) to
/// ν(block_tree). Including it in the hash makes the link cryptographically binding.
pub fn anchor_leaf(nullifier: &Nullifier, contract_id: &ContractId, contract_root: &MerkleNode) -> pallas::Base {
    crate::crypto::poseidon_hash([
        super::constants::DRK_POSEIDON_DOMAIN_MERKLE_LEAF,
        nullifier.inner(),
        contract_id.inner(),
        contract_root.inner(),
    ])
}

/// Nullifier-keyed anchor entry for block tree storage.
///
/// ρ-calculus: `quote((contract_id, contract_root), nullifier)` —
/// the nullifier IS the position key. This struct encodes the data
/// stored as a leaf in the block-level Merkle tree.
///
/// Encoding format (96 bytes):
/// ```text
/// [nullifier: 32B] [contract_id: 32B] [contract_root: 32B]
/// ```
#[derive(Debug, Clone)]
pub struct AnchorEntry {
    pub nullifier: Nullifier,
    pub contract_id: ContractId,
    pub contract_root: MerkleNode,
}

impl AnchorEntry {
    /// Create a new anchor entry. The nullifier links the contract-local
    /// Merkle proof to the block-level Merkle proof.
    pub fn new(
        nullifier: Nullifier,
        contract_id: ContractId,
        contract_root: MerkleNode,
    ) -> Self {
        Self { nullifier, contract_id, contract_root }
    }

    /// Encode as a 96-byte Merkle tree leaf value.
    pub fn to_leaf_bytes(&self) -> [u8; 96] {
        let mut buf = [0u8; 96];
        buf[0..32].copy_from_slice(&self.nullifier.to_bytes());
        buf[32..64].copy_from_slice(&self.contract_id.to_bytes());
        buf[64..96].copy_from_slice(&self.contract_root.to_bytes());
        buf
    }

    /// Decode from a 96-byte Merkle tree leaf value.
    pub fn from_leaf_bytes(bytes: &[u8; 96]) -> Result<Self, ContractError> {
        let nullifier = Nullifier::from_bytes(
            bytes[0..32].try_into().map_err(|_| {
                ContractError::IoError("AnchorEntry: nullifier conversion".into())
            })?,
        )?;
        let contract_id = ContractId::from_bytes(
            bytes[32..64].try_into().map_err(|_| {
                ContractError::IoError("AnchorEntry: contract_id conversion".into())
            })?,
        )?;
        let contract_root = MerkleNode::from_bytes(
            bytes[64..96].try_into().map_err(|_| {
                ContractError::IoError("AnchorEntry: contract_root conversion".into())
            })?,
        )
        .ok_or_else(|| ContractError::IoError("AnchorEntry: invalid contract_root".into()))?;
        Ok(Self { nullifier, contract_id, contract_root })
    }
}

/// Constructs a contract-level Merkle proof for a state transition.
///
/// Returns the Merkle path proving inclusion of the consumed leaf at
/// `leaf_position` in `contract_tree`, and the tree root.
///
/// The block-level proof is deferred (requires Phase 3 block tree in CChainState).
pub fn construct_anchored_proof(
    contract_tree: &crate::crypto::MerkleTree,
    leaf_position: u32,
) -> Result<(MerklePath, MerkleNode), ContractError> {
    let pos_u64 = u64::from(leaf_position);
    let path: Vec<MerkleNode> = contract_tree
        .witness(pos_u64.into(), 0)
        .map_err(|e| ContractError::IoError(format!("construct_anchored_proof: witness failed: {:?}", e)))?;
    let root = contract_tree
        .root(0)
        .ok_or_else(|| ContractError::IoError("construct_anchored_proof: root failed".into()))?;
    let path_array: MerklePath = path
        .try_into()
        .map_err(|_| ContractError::IoError("construct_anchored_proof: path conversion failed".into()))?;
    Ok((path_array, root))
}

/// Anchors a contract-local Merkle tree root into the block-level Merkle tree.
///
/// The nullifier links the two levels. This function SHALL be called during
/// `process_update` (apply), after the contract-local `merkle_add`.
///
/// ρ-calculus: `ν(block_tree).ν(contract_tree).P` — the inner restriction's
/// root is a leaf in the outer restriction. The nullifier is the extruded name.
///
/// TODO: requires `merkle_anchor_add` host function (Phase 4).
/// When the host function exists, this will write `(nullifier, contract_root)`
/// as a leaf in the block-level Merkle tree.
pub fn anchor_contract_root(
    _contract_id: &ContractId,
    _nullifier: &Nullifier,
    _contract_root: &MerkleNode,
) -> Result<(), ContractError> {
    // TODO: requires merkle_anchor_add host function (Phase 4)
    // let anchor_bytes = AnchorEntry::new(*nullifier, *contract_id, *contract_root).to_leaf_bytes();
    // wasm::merkle::merkle_anchor_add(&anchor_bytes)?;
    Ok(())
}

/// Verifies a two-level Merkle proof chain:
///
/// ```text
/// nullifier → contract proof → contract root → block proof → block header
/// ```
///
/// Returns `Ok(true)` if both proofs verify against their respective roots.
pub fn verify_anchored_proof(
    block_header_root: &MerkleNode,
    nullifier: &Nullifier,
    contract_proof: &MerklePath,
    contract_leaf: &MerkleNode,
    contract_root: &MerkleNode,
    block_proof: &MerklePath,
    contract_id: &ContractId,
) -> Result<bool, ContractError> {
    // 1. Verify contract-level proof: leaf exists at contract_root
    let mut current = *contract_leaf;
    for sibling in contract_proof.iter() {
        let hash_input = if current.inner() <= sibling.inner() {
            [current.inner(), sibling.inner()]
        } else {
            [sibling.inner(), current.inner()]
        };
        current = MerkleNode::from_base(crate::crypto::poseidon_hash(hash_input));
    }
    if current != *contract_root {
        return Ok(false);
    }

    // 2. Verify block-level proof: anchor_leaf exists at block_header_root
    let anchor = anchor_leaf(nullifier, contract_id, contract_root);
    let anchor_node = MerkleNode::from_base(anchor);
    let mut current = anchor_node;
    for sibling in block_proof.iter() {
        let hash_input = if current.inner() <= sibling.inner() {
            [current.inner(), sibling.inner()]
        } else {
            [sibling.inner(), current.inner()]
        };
        current = MerkleNode::from_base(crate::crypto::poseidon_hash(hash_input));
    }
    if current != *block_header_root {
        return Ok(false);
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::pasta_prelude::PrimeField;
    fn dummy_contract_id() -> ContractId {
        ContractId::from_base(pallas::Base::from(42))
    }

    fn dummy_nullifier() -> Nullifier {
        Nullifier::from_bytes(pallas::Base::from(99u64).to_repr())
            .expect("valid nullifier")
    }

    #[test]
    fn test_anchor_entry_roundtrip() {
        let cid = dummy_contract_id();
        let nf = dummy_nullifier();
        let root = MerkleNode::from_base(pallas::Base::from(1u64));

        let entry = AnchorEntry::new(nf, cid, root);
        let bytes = entry.to_leaf_bytes();
        let decoded = AnchorEntry::from_leaf_bytes(&bytes).expect("roundtrip");

        assert_eq!(decoded.nullifier.to_bytes(), nf.to_bytes());
        assert_eq!(decoded.contract_id.to_bytes(), cid.to_bytes());
        assert_eq!(decoded.contract_root.to_bytes(), root.to_bytes());
    }

    #[test]
    fn test_anchor_leaf_deterministic() {
        let cid = dummy_contract_id();
        let nf = dummy_nullifier();
        let root = MerkleNode::from_base(pallas::Base::from(1u64));

        let a = anchor_leaf(&nf, &cid, &root);
        let b = anchor_leaf(&nf, &cid, &root);
        assert_eq!(a, b, "anchor_leaf must be deterministic");
    }
}
