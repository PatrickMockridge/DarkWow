pub mod entrypoint;
pub mod error;
pub mod model;

use dwow_sdk::define_contract_function;

/// Deterministic ZK mode — TEST-ONLY. `enable_deterministic_zk()` swaps `OsRng` for a
/// seeded RNG so PI-7 determinism can be checked. SECURITY: seed 0 disables zero-knowledge,
/// so the mode is gated behind the `deterministic-zk` cargo feature (harness-only). The
/// wallet/WASM never enable it; `deterministic_zk_enabled()` returns false there.
#[cfg(feature = "deterministic-zk")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "deterministic-zk")]
static DETERMINISTIC_ZK: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "deterministic-zk")]
pub fn enable_deterministic_zk() {
    DETERMINISTIC_ZK.store(true, Ordering::SeqCst);
}

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

define_contract_function!(MultiSigFunction {
    InitializeV1 = 0x00,
    CreateGroupV1 = 0x01,
    SignV1 = 0x02,
    FinalizeV1 = 0x03,
});

pub const MULTISIG_CONTRACT_GROUPS_TREE: &str = "groups";
pub const MULTISIG_CONTRACT_SIGNATURES_TREE: &str = "signatures";
pub const MULTISIG_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";

// V2 circuit namespaces (domain separation, HAZOP RC3)
pub const MULTISIG_CONTRACT_ZKAS_CREATE_GROUP_NS_V2: &str = "CreateGroupV2";
pub const MULTISIG_CONTRACT_ZKAS_SIGN_NS_V2: &str = "SignV2";
pub const MULTISIG_CONTRACT_ZKAS_FINALIZE_NS_V2: &str = "FinalizeV2";
