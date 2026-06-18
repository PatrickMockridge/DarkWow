//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! Compiled only when `feature = "client"` is enabled. See native_token zkbins.rs
//! for full explanation of the two-location pattern (inherited from upstream).

pub const BEARER_BOND_CONTRACT_ZKAS_BURN_V1_BIN: &[u8] =
    include_bytes!("../../proof/burn_v1.zk.bin");
pub const BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_V1_BIN: &[u8] =
    include_bytes!("../../proof/blind_output_v1.zk.bin");
pub const BEARER_BOND_CONTRACT_ZKAS_REDEEM_V1_BIN: &[u8] =
    include_bytes!("../../proof/redeem_v1.zk.bin");
pub const BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_V1_BIN: &[u8] =
    include_bytes!("../../proof/prove_coverage_v1.zk.bin");
