use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, Nullifier, PublicKey},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
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
#[derive(Debug, Clone)]
pub struct Purse {
    pub version: u8,
    pub purse_id: PurseId,
    pub token_commit: pallas::Base,
    pub balance_commit: pallas::Point,
    pub owner_commit: pallas::Base,
}

impl Purse {
    /// Canonical byte size of an encoded Purse.
    pub const ENCODED_SIZE: usize = 129; // 1 + 32 + 32 + 32 + 32

    /// Encode to canonical fixed-offset bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.purse_id.to_bytes());
        buf.extend_from_slice(&self.token_commit.to_repr());
        buf.extend_from_slice(&self.balance_commit.to_bytes());
        buf.extend_from_slice(&self.owner_commit.to_repr());
        buf
    }

    /// Decode from canonical fixed-offset bytes (ρ-calculus: eval).
    /// Every field validates through its named constructor.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Purse: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let version = data[0];
        let purse_id = PurseId::from_bytes(data[1..33].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("Purse: invalid purse_id".into()))?;
        let token_commit = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[33..65].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Purse: invalid token_commit".into()))?;
        let balance_commit = Option::<pallas::Point>::from(
            pallas::Point::from_bytes(data[65..97].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Purse: invalid balance_commit".into()))?;
        let owner_commit = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[97..129].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Purse: invalid owner_commit".into()))?;
        Ok(Purse { version, purse_id, token_commit, balance_commit, owner_commit })
    }
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
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
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
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
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
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}
