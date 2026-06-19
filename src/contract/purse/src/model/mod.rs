use dwow_sdk::pasta::pallas;
use dwow_serial::{SerialDecodable, SerialEncodable};

/// On-chain Purse record.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Purse {
    pub version: u8,
    pub purse_id: pallas::Base,
    pub token_commit: pallas::Base,
    pub balance_commit: pallas::Point,
    pub owner_commit: pallas::Base,
}

/// Deposit parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DepositParamsV1 {
    pub purse_id: pallas::Base,
    pub deposit_amount: u64,
    pub old_balance_commit: pallas::Point,
    pub new_balance_commit: pallas::Point,
    pub owner_pub: pallas::Base,
    pub proof: Vec<u8>,
}

/// Deposit update.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DepositUpdateV1 {
    pub purse_id: pallas::Base,
    pub new_balance_commit: pallas::Point,
    pub deposit_amount: u64,
}

/// Withdraw parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawParamsV1 {
    pub purse_id: pallas::Base,
    pub withdraw_amount: u64,
    pub old_balance_commit: pallas::Point,
    pub new_balance_commit: pallas::Point,
    pub nullifier: pallas::Base,
    pub owner_pub: pallas::Base,
    pub proof: Vec<u8>,
}

/// Withdraw update.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawUpdateV1 {
    pub purse_id: pallas::Base,
    pub nullifier: pallas::Base,
    pub new_balance_commit: pallas::Point,
    pub withdraw_amount: u64,
}

/// Balance parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BalanceParamsV1 {
    pub purse_id: pallas::Base,
    pub token_id: pallas::Base,
    pub balance_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub owner_pub: pallas::Base,
    pub proof: Vec<u8>,
}
