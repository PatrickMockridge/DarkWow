use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, Nullifier, PublicKey},
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

/// MultiSig group unique identifier.
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct GroupId(pub pallas::Base);

impl GroupId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(GroupId)
    }
}

/// On-chain record for a MultiSig group (N-of-M threshold).
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MultiSigGroup {
    pub version: u8,
    pub group_id: GroupId,
    pub pubkeys: Vec<PublicKey>,
    pub threshold: u8,
    pub total_keys: u8,
}

impl MultiSigGroup {
    /// Derive group_id from first pubkey, threshold, and key count.
    pub fn derive_group_id(first_pk: &PublicKey, threshold: u8, total_keys: u8) -> GroupId {
        let (x, y) = first_pk.xy().expect("pk not identity");
        GroupId(poseidon_hash([x, y, pallas::Base::from(threshold as u64), pallas::Base::from(total_keys as u64)]))
    }
}

/// On-chain record for a partial signature.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PartialSignature {
    pub group_id: GroupId,
    pub message_hash: pallas::Base,
    pub nullifier: Nullifier,
}

// ============================================================================
// CreateGroupV1
// ============================================================================

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateGroupParamsV1 {
    pub pubkeys: Vec<PublicKey>,
    pub threshold: u8,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateGroupUpdateV1 {
    pub group_id: GroupId,
    pub pubkeys: Vec<PublicKey>,
    pub threshold: u8,
    pub total_keys: u8,
}

// ============================================================================
// SignV1
// ============================================================================

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SignParamsV1 {
    pub group_id: GroupId,
    pub message_hash: pallas::Base,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SignUpdateV1 {
    pub group_id: GroupId,
    pub message_hash: pallas::Base,
    pub nullifier: Nullifier,
}

// ============================================================================
// FinalizeV1
// ============================================================================

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FinalizeParamsV1 {
    pub group_id: GroupId,
    pub message_hash: pallas::Base,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FinalizeUpdateV1 {
    pub group_id: GroupId,
    pub message_hash: pallas::Base,
    pub approval_commit: pallas::Base,
    pub consumed_nullifiers: Vec<Nullifier>,
}
