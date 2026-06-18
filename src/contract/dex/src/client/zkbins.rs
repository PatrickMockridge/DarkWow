//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const ACCEPT_SWAP_V1_BIN: &[u8] = include_bytes!("../../proof/accept_swap_v1.zk.bin");
pub const CANCEL_SWAP_V1_BIN: &[u8] = include_bytes!("../../proof/cancel_swap_v1.zk.bin");
pub const CREATE_SWAP_V1_BIN: &[u8] = include_bytes!("../../proof/create_swap_v1.zk.bin");
pub const EXECUTE_SWAP_FEE_V1_BIN: &[u8] = include_bytes!("../../proof/execute_swap_fee_v1.zk.bin");
pub const EXECUTE_SWAP_SLIPPAGE_V1_BIN: &[u8] = include_bytes!("../../proof/execute_swap_slippage_v1.zk.bin");
pub const EXECUTE_SWAP_V1_BIN: &[u8] = include_bytes!("../../proof/execute_swap_v1.zk.bin");
