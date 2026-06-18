//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const PURCHASE_COVERAGE_WITH_CAPABILITY_V1_BIN: &[u8] = include_bytes!("../../proof/purchase_coverage_with_capability_v1.zk.bin");
pub const UNDERWRITE_WITH_CAPABILITY_V1_BIN: &[u8] = include_bytes!("../../proof/underwrite_with_capability_v1.zk.bin");
