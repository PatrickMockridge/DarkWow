use dwow_sdk::{
    blockchain::SerializedLen,
    crypto::{pasta_prelude::PrimeField, poseidon_hash, Nullifier, PublicKey},
    error::ContractError,
    pasta::pallas,
};

// ============================================================================
// TOTAL BYTE READS
// ============================================================================
//
// Every `decode` below validates its buffer length before slicing, which makes each `data[a..b]`
// *provably* in-bounds. But provable is not free: the bounds check still compiles, and its panic
// location — `Location { file: &'static str, line: u32 }` — is a string and an integer in the
// contract artifact's data section, which neither `strip` nor `--release` removes. `get` +
// `try_into` removes the check rather than asserting it away, and `saturating_add` keeps the offset
// arithmetic itself total. Copied from native_token's model, which holds the same family.

/// Read exactly `N` bytes at `offset` — total: `get` + `try_into`, no index and no unwrap.
pub(crate) fn read_field<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N], ContractError> {
    data.get(offset..offset.saturating_add(N))
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| {
            ContractError::IoError(format!(
                "truncated field: need {N} bytes at offset {offset}, buffer has {}",
                data.len()
            ))
        })
}

/// Read exactly one byte at `offset` — total, for the same reason as [`read_field`].
pub(crate) fn read_byte(data: &[u8], offset: usize) -> Result<u8, ContractError> {
    data.get(offset).copied().ok_or_else(|| {
        ContractError::IoError(format!(
            "truncated byte at offset {offset}, buffer has {}",
            data.len()
        ))
    })
}

/// Borrow exactly `len` bytes at `offset` — total, for the same reason as [`read_field`].
pub(crate) fn read_slice(data: &[u8], offset: usize, len: usize) -> Result<&[u8], ContractError> {
    data.get(offset..offset.saturating_add(len)).ok_or_else(|| {
        ContractError::IoError(format!(
            "truncated field: need {len} bytes at offset {offset}, buffer has {}",
            data.len()
        ))
    })
}

/// MultiSig group unique identifier.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GroupId(pub pallas::Base);

impl GroupId {
    pub const ENCODED_SIZE: usize = 32;
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(GroupId)
    }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("GroupId: expected 32 bytes, got {}", data.len()))); }
        Self::from_bytes(&read_field::<32>(data, 0)?).ok_or_else(|| ContractError::IoError("GroupId: invalid".into()))
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
    /// Layout: version(1) + group_id(32) + pubkey_count(SerializedLen, 4) + N*pubkey(32) + threshold(1) + total_keys(1)
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        let n = SerializedLen::try_from_len(self.pubkeys.len())?;
        let cap = 39 + self.pubkeys.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.version);
        buf.extend_from_slice(&self.group_id.to_bytes());
        buf.extend_from_slice(&n.to_le_bytes());
        for pk in &self.pubkeys {
            buf.extend_from_slice(&pk.to_bytes());
        }
        buf.push(self.threshold);
        buf.push(self.total_keys);
        Ok(buf)
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 39 {
            return Err(ContractError::IoError(format!(
                "MultiSigGroup: expected >= 39 bytes, got {}", data.len()
            )));
        }
        let version = read_byte(data, 0)?;
        let group_id = GroupId::from_bytes(&read_field::<32>(data, 1)?)
            .ok_or_else(|| ContractError::IoError("MultiSigGroup: invalid group_id".into()))?;
        let pk_count = SerializedLen::from_le_bytes(read_field::<4>(data, 33)?).to_usize();
        let expected = 39 + pk_count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "MultiSigGroup: expected {} bytes for {} pubkeys, got {}",
                expected, pk_count, data.len()
            )));
        }
        let mut pubkeys = Vec::with_capacity(pk_count);
        for i in 0..pk_count {
            let start = 37 + i * 32;
            let pk = PublicKey::from_bytes(read_field::<32>(data, start)?)
                .map_err(|e| ContractError::IoError(format!(
                    "MultiSigGroup: invalid pubkey[{}]: {}", i, e
                )))?;
            pubkeys.push(pk);
        }
        let threshold = read_byte(data, 37 + pk_count * 32)?;
        let total_keys = read_byte(data, 38 + pk_count * 32)?;
        Ok(MultiSigGroup { version, group_id, pubkeys, threshold, total_keys })
    }
}

impl MultiSigGroup {
    /// Derive group_id from first pubkey, threshold, and key count.
    ///
    /// Returns a typed error rather than panicking. The `#[expect]` this replaces read
    /// "PublicKey constructor rejects identity, so xy()/x()/y() is always Some" — which does not
    /// hold for the value its caller passes: the pubkeys arrive from
    /// `CreateGroupParamsV1::decode`, and `PublicKey`'s derived `Decodable` builds the point
    /// directly, never calling `from_bytes`, so a decoded key CAN be the identity (see
    /// `sdk::crypto::keypair::tests::decoded_public_key_can_be_the_identity`).
    pub fn derive_group_id(
        first_pk: &PublicKey,
        threshold: u8,
        total_keys: u8,
    ) -> Result<GroupId, ContractError> {
        let Some((x, y)) = first_pk.xy() else {
            return Err(ContractError::IoError(
                "derive_group_id: first pubkey is the identity point".to_string(),
            ))
        };
        Ok(GroupId(poseidon_hash([
            x, y,
            pallas::Base::from(threshold as u64), pallas::Base::from(total_keys as u64),
        ])))
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
        let group_id = GroupId::from_bytes(&read_field::<32>(data, 0)?)
            .ok_or_else(|| ContractError::IoError("PartialSignature: invalid group_id".into()))?;
        let message_hash = Option::<pallas::Base>::from(
            pallas::Base::from_repr(read_field::<32>(data, 32)?)
        ).ok_or_else(|| ContractError::IoError("PartialSignature: invalid message_hash".into()))?;
        let nullifier = Nullifier::from_bytes(read_field::<32>(data, 64)?)
            .map_err(|e| ContractError::IoError(format!("PartialSignature: invalid nullifier: {}", e)))?;
        Ok(PartialSignature { group_id, message_hash, nullifier })
    }
}

// ============================================================================
// CreateGroupV1
// ============================================================================

#[derive(Debug, Clone,)]
pub struct CreateGroupParamsV1 {
    pub pubkeys: Vec<PublicKey>,
    pub threshold: u8,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for CreateGroupParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CreateGroupParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CreateGroupParamsV1 { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = SerializedLen::try_from_len(self.pubkeys.len())?; let pl = SerializedLen::try_from_len(self.proof.len())?; let mut b = Vec::with_capacity(73+self.pubkeys.len()*32+self.proof.len()); b.extend_from_slice(&n.to_le_bytes()); for pk in &self.pubkeys { b.extend_from_slice(&pk.to_bytes()); } b.push(self.threshold); b.extend_from_slice(&pl.to_le_bytes()); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 73 { return Err(ContractError::IoError("CreateGroupParamsV1: too short".into())); } let pk_count = SerializedLen::from_le_bytes(read_field::<4>(data, 0)?).to_usize(); let mut pos = 4+pk_count*32; if data.len() < pos+2 { return Err(ContractError::IoError("CreateGroupParamsV1: truncated".into())); } let mut pubkeys = Vec::with_capacity(pk_count); for i in 0..pk_count { pubkeys.push(PublicKey::from_bytes(read_field::<32>(data, 4+i*32)?).map_err(|e| ContractError::IoError(format!("CreateGroupParamsV1: invalid pubkey[{}]: {}", i, e)))?); } let threshold = read_byte(data, pos)?; pos += 1; let proof_len = SerializedLen::from_le_bytes(read_field::<4>(data, pos)?).to_usize(); pos += 4; if data.len() != pos+proof_len+64 { return Err(ContractError::IoError(format!("CreateGroupParamsV1: expected {} bytes, got {}", pos+proof_len+64, data.len()))); } let proof = read_slice(data, pos, proof_len)?.to_vec(); pos += proof_len; let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, pos)?)).ok_or_else(|| ContractError::IoError("CreateGroupParamsV1: invalid tx_binding".into()))?; let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, pos+32)?)).ok_or_else(|| ContractError::IoError("CreateGroupParamsV1: invalid tx_nonce".into()))?; Ok(CreateGroupParamsV1 { pubkeys, threshold, proof, tx_binding, tx_nonce }) } }

#[derive(Debug, Clone,)] pub struct CreateGroupUpdateV1 { pub group_id: GroupId, pub pubkeys: Vec<PublicKey>, pub threshold: u8, pub total_keys: u8 }
impl dwow_serial::Encodable for CreateGroupUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CreateGroupUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CreateGroupUpdateV1 { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = SerializedLen::try_from_len(self.pubkeys.len())?; let mut b = Vec::with_capacity(38+self.pubkeys.len()*32); b.extend_from_slice(&self.group_id.encode()); b.extend_from_slice(&n.to_le_bytes()); for pk in &self.pubkeys { b.extend_from_slice(&pk.to_bytes()); } b.push(self.threshold); b.push(self.total_keys); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 38 { return Err(ContractError::IoError("CreateGroupUpdateV1: too short".into())); } let group_id = GroupId::decode(read_slice(data, 0, 32)?)?; let pk_count = SerializedLen::from_le_bytes(read_field::<4>(data, 32)?).to_usize(); let expected = 38+pk_count*32; if data.len() != expected { return Err(ContractError::IoError(format!("CreateGroupUpdateV1: expected {} bytes, got {}", expected, data.len()))); } let mut pubkeys = Vec::with_capacity(pk_count); for i in 0..pk_count { pubkeys.push(PublicKey::from_bytes(read_field::<32>(data, 36+i*32)?).map_err(|e| ContractError::IoError(format!("CreateGroupUpdateV1: invalid pubkey[{}]: {}", i, e)))?); } Ok(CreateGroupUpdateV1 { group_id, pubkeys, threshold: read_byte(data, 37+pk_count*32)?, total_keys: read_byte(data, 38+pk_count*32)? }) } }

// SignV1
#[derive(Debug, Clone,)] pub struct SignParamsV1 { pub group_id: GroupId, pub message_hash: pallas::Base, pub signer_pub: PublicKey, pub proof: Vec<u8>, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base }
impl dwow_serial::Encodable for SignParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for SignParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl SignParamsV1 { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let pl = SerializedLen::try_from_len(self.proof.len())?; let mut b = Vec::with_capacity(100+self.proof.len()); b.extend_from_slice(&self.group_id.encode()); b.extend_from_slice(&self.message_hash.to_repr()); b.extend_from_slice(&self.signer_pub.to_bytes()); b.extend_from_slice(&pl.to_le_bytes()); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 100 { return Err(ContractError::IoError("SignParamsV1: too short".into())); } let group_id = GroupId::decode(read_slice(data, 0, 32)?)?; let message_hash = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 32)?)).ok_or_else(|| ContractError::IoError("SignParamsV1: invalid message_hash".into()))?; let signer_pub = PublicKey::from_bytes(read_field::<32>(data, 64)?).map_err(|e| ContractError::IoError(format!("SignParamsV1: invalid signer_pub: {}", e)))?; let proof_len = SerializedLen::from_le_bytes(read_field::<4>(data, 96)?).to_usize(); if data.len() != 100+proof_len+64 { return Err(ContractError::IoError(format!("SignParamsV1: expected {} bytes, got {}", 100+proof_len+64, data.len()))); } let proof = read_slice(data, 100, proof_len)?.to_vec(); let pos = 100+proof_len; let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, pos)?)).ok_or_else(|| ContractError::IoError("SignParamsV1: invalid tx_binding".into()))?; let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, pos+32)?)).ok_or_else(|| ContractError::IoError("SignParamsV1: invalid tx_nonce".into()))?; Ok(SignParamsV1 { group_id, message_hash, signer_pub, proof, tx_binding, tx_nonce }) } }

#[derive(Debug, Clone,)] pub struct SignUpdateV1 { pub group_id: GroupId, pub message_hash: pallas::Base, pub nullifier: Nullifier }
impl dwow_serial::Encodable for SignUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for SignUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl SignUpdateV1 { pub const ENCODED_SIZE: usize = 96; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(96); b.extend_from_slice(&self.group_id.encode()); b.extend_from_slice(&self.message_hash.to_repr()); b.extend_from_slice(&self.nullifier.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 96 { return Err(ContractError::IoError(format!("SignUpdateV1: expected 96 bytes, got {}", data.len()))); } Ok(SignUpdateV1 { group_id: GroupId::decode(read_slice(data, 0, 32)?)?, message_hash: Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 32)?)).ok_or_else(|| ContractError::IoError("SignUpdateV1: invalid message_hash".into()))?, nullifier: Nullifier::from_bytes(read_field::<32>(data, 64)?)? }) } }

// FinalizeV1
#[derive(Debug, Clone,)] pub struct FinalizeParamsV1 { pub group_id: GroupId, pub message_hash: pallas::Base, pub proof: Vec<u8>, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base }
impl dwow_serial::Encodable for FinalizeParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for FinalizeParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl FinalizeParamsV1 { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let pl = SerializedLen::try_from_len(self.proof.len())?; let mut b = Vec::with_capacity(68+self.proof.len()); b.extend_from_slice(&self.group_id.encode()); b.extend_from_slice(&self.message_hash.to_repr()); b.extend_from_slice(&pl.to_le_bytes()); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 68 { return Err(ContractError::IoError("FinalizeParamsV1: too short".into())); } let group_id = GroupId::decode(read_slice(data, 0, 32)?)?; let message_hash = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 32)?)).ok_or_else(|| ContractError::IoError("FinalizeParamsV1: invalid message_hash".into()))?; let proof_len = SerializedLen::from_le_bytes(read_field::<4>(data, 64)?).to_usize(); if data.len() != 68+proof_len+64 { return Err(ContractError::IoError(format!("FinalizeParamsV1: expected {} bytes, got {}", 68+proof_len+64, data.len()))); } let proof = read_slice(data, 68, proof_len)?.to_vec(); let pos = 68+proof_len; let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, pos)?)).ok_or_else(|| ContractError::IoError("FinalizeParamsV1: invalid tx_binding".into()))?; let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, pos+32)?)).ok_or_else(|| ContractError::IoError("FinalizeParamsV1: invalid tx_nonce".into()))?; Ok(FinalizeParamsV1 { group_id, message_hash, proof, tx_binding, tx_nonce }) } }

#[derive(Debug, Clone,)] pub struct FinalizeUpdateV1 { pub group_id: GroupId, pub message_hash: pallas::Base, pub approval_commit: pallas::Base, pub consumed_nullifiers: Vec<Nullifier> }
impl dwow_serial::Encodable for FinalizeUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for FinalizeUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl FinalizeUpdateV1 { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = SerializedLen::try_from_len(self.consumed_nullifiers.len())?; let mut b = Vec::with_capacity(100+self.consumed_nullifiers.len()*32); b.extend_from_slice(&self.group_id.encode()); b.extend_from_slice(&self.message_hash.to_repr()); b.extend_from_slice(&self.approval_commit.to_repr()); b.extend_from_slice(&n.to_le_bytes()); for nf in &self.consumed_nullifiers { b.extend_from_slice(&nf.to_bytes()); } Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 100 { return Err(ContractError::IoError("FinalizeUpdateV1: too short".into())); } let group_id = GroupId::decode(read_slice(data, 0, 32)?)?; let message_hash = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 32)?)).ok_or_else(|| ContractError::IoError("FinalizeUpdateV1: invalid message_hash".into()))?; let approval_commit = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 64)?)).ok_or_else(|| ContractError::IoError("FinalizeUpdateV1: invalid approval_commit".into()))?; let nf_count = SerializedLen::from_le_bytes(read_field::<4>(data, 96)?).to_usize(); let expected = 100+nf_count*32; if data.len() != expected { return Err(ContractError::IoError(format!("FinalizeUpdateV1: expected {} bytes, got {}", expected, data.len()))); } let mut consumed_nullifiers = Vec::with_capacity(nf_count); for i in 0..nf_count { consumed_nullifiers.push(Nullifier::from_bytes(read_field::<32>(data, 100+i*32)?)?); } Ok(FinalizeUpdateV1 { group_id, message_hash, approval_commit, consumed_nullifiers }) } }
