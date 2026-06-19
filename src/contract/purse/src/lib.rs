pub mod entrypoint;
pub mod error;
pub mod model;

use dwow_sdk::define_contract_function;

define_contract_function!(PurseFunction {
    InitializeV1 = 0x00,
    DepositV1 = 0x01,
    WithdrawV1 = 0x02,
    BalanceV1 = 0x03,
});

pub const PURSE_CONTRACT_DB_VERSION: &[u8] = b"purse_version_1";
pub const PURSE_CONTRACT_PURSES_TREE: &[u8] = b"purses";
pub const PURSE_CONTRACT_NULLIFIERS_TREE: &[u8] = b"nullifiers";
pub const PURSE_CONTRACT_INFO_TREE: &[u8] = b"info";
pub const PURSE_CONTRACT_ZKAS_DEPOSIT_NS_V1: &str = "DepositV1";
pub const PURSE_CONTRACT_ZKAS_WITHDRAW_NS_V1: &str = "WithdrawV1";
pub const PURSE_CONTRACT_ZKAS_BALANCE_NS_V1: &str = "BalanceV1";
