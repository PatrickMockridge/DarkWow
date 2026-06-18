//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const ADD_LIQUIDITY_V1_BIN: &[u8] = include_bytes!("../../proof/add_liquidity_v1.zk.bin");
pub const BUY_POSITION_V1_BIN: &[u8] = include_bytes!("../../proof/buy_position_v1.zk.bin");
pub const CLAIM_WINNINGS_V1_BIN: &[u8] = include_bytes!("../../proof/claim_winnings_v1.zk.bin");
pub const CREATE_MARKET_V1_BIN: &[u8] = include_bytes!("../../proof/create_market_v1.zk.bin");
