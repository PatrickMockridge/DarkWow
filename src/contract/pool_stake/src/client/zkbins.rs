//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const ALLOCATE_COVERAGE_V1_BIN: &[u8] = include_bytes!("../../proof/allocate_coverage_v1.zk.bin");
pub const CREATE_POOL_V1_BIN: &[u8] = include_bytes!("../../proof/create_pool_v1.zk.bin");
pub const JOIN_POOL_V1_BIN: &[u8] = include_bytes!("../../proof/join_pool_v1.zk.bin");
pub const SLASH_COVERAGE_V1_BIN: &[u8] = include_bytes!("../../proof/slash_coverage_v1.zk.bin");
