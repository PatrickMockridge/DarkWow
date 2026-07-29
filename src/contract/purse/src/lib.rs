pub mod entrypoint;
pub mod error;
pub mod model;

use dwow_sdk::define_contract_function;

define_contract_function!(PurseFunction {
    InitializeV1 = 0x00,
    DepositV1 = 0x01,
    WithdrawV1 = 0x02,
    BalanceV1 = 0x03,
    DepositV3 = 0x04,
    WithdrawV3 = 0x05,
    BalanceV3 = 0x06,
});

pub const PURSE_CONTRACT_DB_VERSION: &str = "purse_version_1";
pub const PURSE_CONTRACT_PURSES_TREE: &str = "purses";
pub const PURSE_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const PURSE_CONTRACT_INFO_TREE: &str = "info";

// V1 circuit namespaces
pub const PURSE_CONTRACT_ZKAS_DEPOSIT_NS_V1: &str = "DepositV1";
pub const PURSE_CONTRACT_ZKAS_WITHDRAW_NS_V1: &str = "WithdrawV1";
pub const PURSE_CONTRACT_ZKAS_BALANCE_NS_V1: &str = "BalanceV1";

// V3 circuit namespaces (hard path — Merkle inclusion proofs)
pub const PURSE_CONTRACT_ZKAS_DEPOSIT_NS_V3: &str = "DepositV3";
pub const PURSE_CONTRACT_ZKAS_WITHDRAW_NS_V3: &str = "WithdrawV3";
pub const PURSE_CONTRACT_ZKAS_BALANCE_NS_V3: &str = "BalanceV3";

// Merkle tree infrastructure
pub const PURSE_CONTRACT_PURSE_ROOTS_TREE: &str = "purse_roots";
pub const PURSE_CONTRACT_NULLIFIER_ROOTS_TREE: &str = "nullifier_roots";
pub const PURSE_CONTRACT_PURSE_MERKLE_TREE: &[u8] = b"purse_merkle_tree";
pub const PURSE_CONTRACT_LATEST_PURSE_ROOT: &[u8] = b"latest_purse_root";
pub const PURSE_CONTRACT_LATEST_NULLIFIER_ROOT: &[u8] = b"latest_nullifier_root";

// Precalculated root for MerkleTree::new(1) with single ZERO leaf.
// Identical to PromissoryNote's EMPTY_COINS_TREE_ROOT / Box's EMPTY_BOX_TREE_ROOT.
pub const EMPTY_PURSE_TREE_ROOT: [u8; 32] = [
    0xb8, 0xc1, 0x07, 0x5a, 0x80, 0xa8, 0x09, 0x65, 0xc2, 0x39, 0x8f, 0x71, 0x1f, 0xe7, 0x3e, 0x05,
    0xb4, 0xed, 0xae, 0xde, 0xf1, 0x62, 0xf2, 0x61, 0xd4, 0xee, 0xd7, 0xcd, 0x72, 0x74, 0x8d, 0x17,
];
