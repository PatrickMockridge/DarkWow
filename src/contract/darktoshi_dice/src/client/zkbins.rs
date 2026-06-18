//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const COMMIT_BET_V1_BIN: &[u8] = include_bytes!("../../proof/commit_bet_v1.zk.bin");
pub const SETTLE_BET_V1_BIN: &[u8] = include_bytes!("../../proof/settle_bet_v1.zk.bin");
