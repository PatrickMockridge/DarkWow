//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const CREATE_TENDER_V1_BIN: &[u8] = include_bytes!("../../proof/create_tender_v1.zk.bin");
pub const REVEAL_BID_V1_BIN: &[u8] = include_bytes!("../../proof/reveal_bid_v1.zk.bin");
pub const SELECT_WINNER_V1_BIN: &[u8] = include_bytes!("../../proof/select_winner_v1.zk.bin");
pub const SUBMIT_BID_V1_BIN: &[u8] = include_bytes!("../../proof/submit_bid_v1.zk.bin");
pub const SUBMIT_BID_WITH_CAPABILITY_V1_BIN: &[u8] = include_bytes!("../../proof/submit_bid_with_capability_v1.zk.bin");
