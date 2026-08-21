#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod capability;
pub mod entrypoint;
pub mod error;
pub mod model;

use dwow_sdk::define_contract_function;

define_contract_function!(PurseFunction {
    Initialize = 0x00,
    Deposit = 0x01,
    Withdraw = 0x02,
    Balance = 0x03,
});

pub const PURSE_CONTRACT_DB_VERSION: &str = "purse_version_1";
pub const PURSE_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const PURSE_CONTRACT_INFO_TREE: &str = "info";

pub const PURSE_CONTRACT_ZKAS_DEPOSIT_NS: &str = "Deposit";
pub const PURSE_CONTRACT_ZKAS_WITHDRAW_NS: &str = "Withdraw";
pub const PURSE_CONTRACT_ZKAS_BALANCE_NS: &str = "Balance";

// Merkle tree infrastructure
pub const PURSE_CONTRACT_PURSE_ROOTS_TREE: &str = "purse_roots";
pub const PURSE_CONTRACT_PURSE_MERKLE_TREE: &[u8] = b"purse_merkle_tree";
pub const PURSE_CONTRACT_LATEST_PURSE_ROOT: &[u8] = b"latest_purse_root";

// Precalculated root for MerkleTree::new(1) with single ZERO leaf.
pub const EMPTY_PURSE_TREE_ROOT: [u8; 32] = [
    0xb8, 0xc1, 0x07, 0x5a, 0x80, 0xa8, 0x09, 0x65, 0xc2, 0x39, 0x8f, 0x71, 0x1f, 0xe7, 0x3e, 0x05,
    0xb4, 0xed, 0xae, 0xde, 0xf1, 0x62, 0xf2, 0x61, 0xd4, 0xee, 0xd7, 0xcd, 0x72, 0x74, 0x8d, 0x17,
];

/// Thread-safe flag for deterministic ZK proof generation (DZ-4).
/// Set by tests before genesis to eliminate OsRng from proof generation.
/// Per heavyweight-spec.md §7.4: gated behind the `deterministic-zk` feature
/// (test builds only); wallet/WASM builds get `deterministic_zk_enabled()==false`.
#[cfg(feature = "deterministic-zk")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "deterministic-zk")]
static DETERMINISTIC_ZK: AtomicBool = AtomicBool::new(false);

/// Enable deterministic ZK proof generation for testing.
/// Replaces OsRng with StdRng::seed_from_u64(0).
#[cfg(feature = "deterministic-zk")]
pub fn enable_deterministic_zk() {
    DETERMINISTIC_ZK.store(true, Ordering::SeqCst);
}

/// Returns true if deterministic ZK mode is enabled. Always `false` unless the
/// `deterministic-zk` feature is enabled (test builds only — heavyweight-spec.md §7.4 DZ-4).
pub fn deterministic_zk_enabled() -> bool {
    #[cfg(feature = "deterministic-zk")]
    {
        DETERMINISTIC_ZK.load(Ordering::SeqCst)
    }
    #[cfg(not(feature = "deterministic-zk"))]
    {
        false
    }
}
