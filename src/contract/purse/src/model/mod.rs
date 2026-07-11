use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, Nullifier, PublicKey},
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

/// Purse unique identifier — Poseidon hash of owner and instance data.
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct PurseId(pub pallas::Base);

impl PurseId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(PurseId)
    }
}

/// On-chain Purse record.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Purse {
    pub version: u8,
    pub purse_id: PurseId,
    pub token_commit: pallas::Base,
    pub balance_commit: pallas::Point,
    pub owner_commit: pallas::Base,
}

/// Deposit parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DepositParamsV1 {
    pub purse_id: PurseId,
    pub deposit_amount: u64,
    pub old_balance_commit: pallas::Point,
    pub new_balance_commit: pallas::Point,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
}

/// Deposit update.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DepositUpdateV1 {
    pub purse_id: PurseId,
    pub new_balance_commit: pallas::Point,
    pub deposit_amount: u64,
}

/// Withdraw parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawParamsV1 {
    pub purse_id: PurseId,
    pub withdraw_amount: u64,
    pub old_balance_commit: pallas::Point,
    pub new_balance_commit: pallas::Point,
    pub nullifier: Nullifier,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
}

/// Withdraw update.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawUpdateV1 {
    pub purse_id: PurseId,
    pub nullifier: Nullifier,
    pub new_balance_commit: pallas::Point,
    pub withdraw_amount: u64,
}

/// Balance parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BalanceParamsV1 {
    pub purse_id: PurseId,
    pub token_id: pallas::Base,
    pub balance_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
}
