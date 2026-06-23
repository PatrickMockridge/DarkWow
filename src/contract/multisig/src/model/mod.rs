use dwow_serial::{Decodable, Encodable};
use pallas::crypto::hash::poseidon_hash;
use pallas::curve::ProjectivePoint;

/// On-chain record for a MultiSig group.
#[derive(Debug, Clone, Encodable, Decodable)]
pub struct MultiSigGroup {
    /// Group identifier: poseidon_hash(pubkeys || threshold)
    pub group_id: pallas::Base,
    /// Compressed public keys of group members
    pub pubkeys: Vec<pallas::Point>,
    /// Required number of signatures (M in N-of-M)
    pub threshold: u8,
    /// Total number of group members (N in N-of-M)
    pub total_keys: u8,
}

impl MultiSigGroup {
    /// Compute the group_id from pubkeys and threshold.
    pub fn compute_group_id(pubkeys: &[pallas::Point], threshold: u8) -> pallas::Base {
        let mut inputs: Vec<pallas::Base> = Vec::with_capacity(pubkeys.len() * 2 + 1);
        for pk in pubkeys {
            inputs.push(pk.get_x());
            inputs.push(pk.get_y());
        }
        inputs.push(pallas::Base::from(threshold as u64));
        poseidon_hash(&inputs)
    }
}

/// On-chain record for a partial signature.
#[derive(Debug, Clone, Encodable, Decodable)]
pub struct PartialSignature {
    /// The group this signature belongs to
    pub group_id: pallas::Base,
    /// Hash of the message being signed
    pub message_hash: pallas::Base,
    /// Public key of the signer
    pub signer_pubkey: pallas::Point,
    /// Nullifier: poseidon_hash(group_id, message_hash, signer_pubkey)
    pub nullifier: pallas::Base,
}

// ============================================================================
// CreateGroupV1
// ============================================================================

#[derive(Debug, Clone, Encodable, Decodable)]
pub struct CreateGroupParamsV1 {
    /// Compressed public keys (32 bytes each)
    pub pubkeys: Vec<[u8; 32]>,
    /// Required number of signatures (M)
    pub threshold: u8,
    /// ZK proof
    pub proof: Vec<u8>,
}

#[derive(Debug, Clone, Encodable, Decodable)]
pub struct CreateGroupUpdateV1 {
    pub group_id: pallas::Base,
    pub pubkeys: Vec<pallas::Point>,
    pub threshold: u8,
    pub total_keys: u8,
}

// ============================================================================
// SignV1
// ============================================================================

#[derive(Debug, Clone, Encodable, Decodable)]
pub struct SignParamsV1 {
    /// Group to sign for
    pub group_id: pallas::Base,
    /// Message to sign (raw bytes, hashed to message_hash)
    pub message: Vec<u8>,
    /// ZK proof
    pub proof: Vec<u8>,
}

#[derive(Debug, Clone, Encodable, Decodable)]
pub struct SignUpdateV1 {
    pub group_id: pallas::Base,
    pub message_hash: pallas::Base,
    pub signer_pubkey: pallas::Point,
    pub nullifier: pallas::Base,
}

// ============================================================================
// FinalizeV1
// ============================================================================

#[derive(Debug, Clone, Encodable, Decodable)]
pub struct FinalizeParamsV1 {
    /// Group to finalize for
    pub group_id: pallas::Base,
    /// Hash of the message being approved
    pub message_hash: pallas::Base,
    /// ZK proof
    pub proof: Vec<u8>,
}

#[derive(Debug, Clone, Encodable, Decodable)]
pub struct FinalizeUpdateV1 {
    pub group_id: pallas::Base,
    pub message_hash: pallas::Base,
    /// Commitment to the approval capability
    pub approval_commit: pallas::Base,
    /// Nullifiers of consumed partial signatures
    pub consumed_nullifiers: Vec<pallas::Base>,
}
