//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const CLAIM_V1_BIN: &[u8] = include_bytes!("../../proof/claim_v1.zk.bin");
pub const INIT_V1_BIN: &[u8] = include_bytes!("../../proof/init_v1.zk.bin");
pub const STAKE_V1_BIN: &[u8] = include_bytes!("../../proof/stake_v1.zk.bin");
pub const UNSTAKE_V1_BIN: &[u8] = include_bytes!("../../proof/unstake_v1.zk.bin");
pub const UPDATE_RISK_V1_BIN: &[u8] = include_bytes!("../../proof/update_risk_v1.zk.bin");
