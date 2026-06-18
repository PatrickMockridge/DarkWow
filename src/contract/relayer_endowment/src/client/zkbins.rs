//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const CLAIM_FEES_V1_BIN: &[u8] = include_bytes!("../../proof/claim_fees_v1.zk.bin");
pub const DEPLOY_CAPITAL_V1_BIN: &[u8] = include_bytes!("../../proof/deploy_capital_v1.zk.bin");
pub const INITIALIZE_V1_BIN: &[u8] = include_bytes!("../../proof/initialize_v1.zk.bin");
