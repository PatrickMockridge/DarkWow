//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const SUBSCRIBE_V1_BIN: &[u8] = include_bytes!("../../proof/subscribe_v1.zk.bin");
pub const UPDATE_USAGE_V1_BIN: &[u8] = include_bytes!("../../proof/update_usage_v1.zk.bin");
pub const VERIFY_ACCESS_V1_BIN: &[u8] = include_bytes!("../../proof/verify_access_v1.zk.bin");
