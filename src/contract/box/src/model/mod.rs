use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, Nullifier, PublicKey},
    error::ContractError,
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

/// Box unique identifier — Poseidon hash of creator public key and nonce.
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PutUpdateV1 {
    pub box_id: BoxId,
    pub new_contents_commit: pallas::Base,
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TakeUpdateV1 {
    pub box_id: BoxId,
    pub nullifier: Nullifier,
}
