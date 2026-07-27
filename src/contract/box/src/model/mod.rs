use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, Nullifier, PublicKey},
    error::ContractError,
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

impl dwow_serial::Encodable for BoxId {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> Result<usize, std::io::Error> {
        let bytes = self.to_bytes();
        w.write_all(&bytes)?;
        Ok(32)
    }
}

impl dwow_serial::Decodable for BoxId {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self, std::io::Error> {
        let mut buf = [0u8; 32];
        d.read_exact(&mut buf)?;
        Self::from_bytes(&buf)
            .ok_or_else(|| std::io::Error::other("BoxId: invalid field element"))
    }
}

/// Box unique identifier — Poseidon hash of creator public key and nonce.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct BoxId(pub pallas::Base);

impl BoxId {
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(BoxId)
    }
}

/// On-chain Box record.
#[derive(Debug, Clone)]
pub struct BoxRecord {
    pub version: u8,
    pub box_id: BoxId,
    pub contents_commit: pallas::Base,
    pub is_empty: bool,
}

impl BoxRecord {
    pub const ENCODED_SIZE: usize = 66; // 1 + 32 + 32 + 1

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.box_id.to_bytes());
        buf.extend_from_slice(&self.contents_commit.to_repr());
        buf.push(self.is_empty as u8);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "BoxRecord: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let version = data[0];
        let box_id = BoxId::from_bytes(data[1..33].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("BoxRecord: invalid box_id".into()))?;
        let contents_commit = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[33..65].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("BoxRecord: invalid contents_commit".into()))?;
        let is_empty = data[65] != 0;
        Ok(BoxRecord { version, box_id, contents_commit, is_empty })
    }
}

/// Put parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PutParamsV1 {
    pub box_id: BoxId,
    pub old_contents_commit: pallas::Base,
    pub new_contents_commit: pallas::Base,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Put update.
#[derive(Debug, Clone)]
pub struct PutUpdateV1 {
    pub box_id: BoxId,
    pub new_contents_commit: pallas::Base,
}

impl PutUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.box_id.to_bytes()); b.extend_from_slice(&self.new_contents_commit.to_repr()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("PutUpdateV1: expected 64 bytes, got {}", data.len()))); } Ok(PutUpdateV1 { box_id: BoxId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("PutUpdateV1: invalid box_id".into()))?, new_contents_commit: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PutUpdateV1: invalid new_contents_commit".into()))? }) }
}

/// Take parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TakeParamsV1 {
    pub box_id: BoxId,
    pub contents_commit: pallas::Base,
    pub nullifier: Nullifier,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Take update.
#[derive(Debug, Clone)]
pub struct TakeUpdateV1 {
    pub box_id: BoxId,
    pub nullifier: Nullifier,
}

impl TakeUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.box_id.to_bytes()); b.extend_from_slice(&self.nullifier.to_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("TakeUpdateV1: expected 64 bytes, got {}", data.len()))); } Ok(TakeUpdateV1 { box_id: BoxId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("TakeUpdateV1: invalid box_id".into()))?, nullifier: Nullifier::from_bytes(data[32..64].try_into().unwrap())? }) }
}
