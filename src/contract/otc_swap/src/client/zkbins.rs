//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const CANCEL_SWAP_V1_BIN: &[u8] = include_bytes!("../../proof/cancel_swap_v1.zk.bin");
pub const CREATE_SWAP_V1_BIN: &[u8] = include_bytes!("../../proof/create_swap_v1.zk.bin");
pub const EXECUTE_SWAP_V1_BIN: &[u8] = include_bytes!("../../proof/execute_swap_v1.zk.bin");
pub const FUND_SWAP_V1_BIN: &[u8] = include_bytes!("../../proof/fund_swap_v1.zk.bin");
