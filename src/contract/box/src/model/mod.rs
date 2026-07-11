use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, Nullifier, PublicKey},
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BoxRecord {
    pub version: u8,
    pub box_id: BoxId,
    pub contents_commit: pallas::Base,
    pub is_empty: bool,
}

/// Put parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PutParamsV1 {
    pub box_id: BoxId,
    pub old_contents_commit: pallas::Base,
    pub new_contents_commit: pallas::Base,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
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
}

/// Take update.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TakeUpdateV1 {
    pub box_id: BoxId,
    pub nullifier: Nullifier,
}
