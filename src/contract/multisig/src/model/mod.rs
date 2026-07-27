use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, Nullifier, PublicKey},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
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
#[derive(Debug, Clone)]
pub struct MultiSigGroup {
    pub version: u8,
    pub group_id: GroupId,
    pub pubkeys: Vec<PublicKey>,
    pub threshold: u8,
    pub total_keys: u8,
}

impl MultiSigGroup {
    /// Encode to canonical bytes (ρ-calculus: quote).
    /// Layout: version(1) + group_id(32) + pubkey_count(u8) + N*pubkey(32) + threshold(1) + total_keys(1)
    pub fn encode(&self) -> Vec<u8> {
        let cap = 35 + self.pubkeys.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.version);
        buf.extend_from_slice(&self.group_id.to_bytes());
        buf.push(self.pubkeys.len() as u8);
        for pk in &self.pubkeys {
            buf.extend_from_slice(&pk.to_bytes());
        }
        buf.push(self.threshold);
        buf.push(self.total_keys);
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 35 {
            return Err(ContractError::IoError(format!(
                "MultiSigGroup: expected >= 35 bytes, got {}", data.len()
            )));
        }
        let version = data[0];
        let group_id = GroupId::from_bytes(data[1..33].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("MultiSigGroup: invalid group_id".into()))?;
        let pk_count = data[33] as usize;
        let expected = 35 + pk_count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "MultiSigGroup: expected {} bytes for {} pubkeys, got {}",
                expected, pk_count, data.len()
            )));
        }
        let mut pubkeys = Vec::with_capacity(pk_count);
        for i in 0..pk_count {
            let start = 34 + i * 32;
            let pk = PublicKey::from_bytes(data[start..start+32].try_into().unwrap())
                .map_err(|e| ContractError::IoError(format!(
                    "MultiSigGroup: invalid pubkey[{}]: {}", i, e
                )))?;
            pubkeys.push(pk);
        }
        let threshold = data[34 + pk_count * 32];
        let total_keys = data[35 + pk_count * 32];
        Ok(MultiSigGroup { version, group_id, pubkeys, threshold, total_keys })
    }
}

impl MultiSigGroup {
    /// Derive group_id from first pubkey, threshold, and key count.
    pub fn derive_group_id(first_pk: &PublicKey, threshold: u8, total_keys: u8) -> GroupId {
        let (x, y) = first_pk.xy().expect("pk not identity");
        GroupId(poseidon_hash([x, y, pallas::Base::from(threshold as u64), pallas::Base::from(total_keys as u64)]))
    }
}

/// On-chain record for a partial signature.
#[derive(Debug, Clone)]
pub struct PartialSignature {
    pub group_id: GroupId,
    pub message_hash: pallas::Base,
    pub nullifier: Nullifier,
}

impl PartialSignature {
    /// Fixed canonical byte size.
    pub const ENCODED_SIZE: usize = 96; // 32 + 32 + 32

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.group_id.to_bytes());
        buf.extend_from_slice(&self.message_hash.to_repr());
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "PartialSignature: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let group_id = GroupId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("PartialSignature: invalid group_id".into()))?;
        let message_hash = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[32..64].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("PartialSignature: invalid message_hash".into()))?;
        let nullifier = Nullifier::from_bytes(data[64..96].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("PartialSignature: invalid nullifier: {}", e)))?;
        Ok(PartialSignature { group_id, message_hash, nullifier })
    }
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
    pub signer_pub: PublicKey,
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
