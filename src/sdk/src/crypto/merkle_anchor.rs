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

/// Computes the anchored leaf value: `poseidon_hash(contract_id || contract_root)`.
///
/// This is the value stored in the block-level Merkle tree, keyed by nullifier.
/// The Poseidon hash commits to both the contract identity and the contract tree
/// root, making the anchor unique per (contract, root) pair.
pub fn anchor_leaf(contract_id: &ContractId, contract_root: &MerkleNode) -> pallas::Base {
    crate::crypto::poseidon_hash([
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
        let root = MerkleNode::from_base(pallas::Base::from(1u64));

        let a = anchor_leaf(&cid, &root);
        let b = anchor_leaf(&cid, &root);
        assert_eq!(a, b, "anchor_leaf must be deterministic");
    }
}
