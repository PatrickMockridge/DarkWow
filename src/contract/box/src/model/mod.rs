use dwow_sdk::pasta::pallas;
use dwow_serial::{SerialDecodable, SerialEncodable};

/// On-chain Box record.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BoxRecord {
    pub version: u8,
    pub box_id: pallas::Base,
    pub contents_commit: pallas::Base,
    pub is_empty: bool,
}

/// Put parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PutParamsV1 {
    pub box_id: pallas::Base,
    pub old_contents_commit: pallas::Base,
    pub new_contents_commit: pallas::Base,
    pub owner_pub_x: pallas::Base,
    pub owner_pub_y: pallas::Base,
    pub proof: Vec<u8>,
}

/// Put update.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PutUpdateV1 {
    pub box_id: pallas::Base,
    pub new_contents_commit: pallas::Base,
}

/// Take parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TakeParamsV1 {
    pub box_id: pallas::Base,
    pub contents_commit: pallas::Base,
    pub nullifier: pallas::Base,
    pub owner_pub_x: pallas::Base,
    pub owner_pub_y: pallas::Base,
    pub proof: Vec<u8>,
}

/// Take update.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TakeUpdateV1 {
    pub box_id: pallas::Base,
    pub nullifier: pallas::Base,
}
