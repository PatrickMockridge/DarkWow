use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

/// On-chain record for a MultiSig group (N-of-M threshold).
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MultiSigGroup {
    pub version: u8,
    pub group_id: pallas::Base,
    pub pubkeys: Vec<PublicKey>,
    pub threshold: u8,
    pub total_keys: u8,
}

impl MultiSigGroup {
    /// Derive group_id from first pubkey, threshold, and key count.
    pub fn derive_group_id(first_pk: &PublicKey, threshold: u8, total_keys: u8) -> pallas::Base {
        // PublicKey constructor rejects identity, so xy() is always Some
        let (x, y) = first_pk.xy().expect("pk not identity");
        poseidon_hash([x, y, pallas::Base::from(threshold as u64), pallas::Base::from(total_keys as u64)])
    }
}

/// On-chain record for a partial signature.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PartialSignature {
    pub group_id: pallas::Base,
    pub message_hash: pallas::Base,
    pub nullifier: pallas::Base,
}

// ============================================================================
// CreateGroupV1
// ============================================================================

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateGroupParamsV1 {
    pub pubkeys: Vec<[u8; 32]>,
    pub threshold: u8,
    pub proof: Vec<u8>,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateGroupUpdateV1 {
    pub group_id: pallas::Base,
    pub pubkeys: Vec<PublicKey>,
    pub threshold: u8,
    pub total_keys: u8,
}

// ============================================================================
// SignV1
// ============================================================================

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SignParamsV1 {
    pub group_id: pallas::Base,
    pub message_hash: pallas::Base,
    pub proof: Vec<u8>,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SignUpdateV1 {
    pub group_id: pallas::Base,
    pub message_hash: pallas::Base,
    pub nullifier: pallas::Base,
}

// ============================================================================
// FinalizeV1
// ============================================================================

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FinalizeParamsV1 {
    pub group_id: pallas::Base,
    pub message_hash: pallas::Base,
    pub proof: Vec<u8>,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FinalizeUpdateV1 {
    pub group_id: pallas::Base,
    pub message_hash: pallas::Base,
    pub approval_commit: pallas::Base,
    pub consumed_nullifiers: Vec<pallas::Base>,
}
