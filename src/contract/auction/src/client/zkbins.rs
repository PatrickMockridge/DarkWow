//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const CLAIM_WINNINGS_V1_BIN: &[u8] = include_bytes!("../../proof/claim_winnings_v1.zk.bin");
pub const CLOSE_AUCTION_V1_BIN: &[u8] = include_bytes!("../../proof/close_auction_v1.zk.bin");
pub const CREATE_AUCTION_V1_BIN: &[u8] = include_bytes!("../../proof/create_auction_v1.zk.bin");
pub const PLACE_BID_V1_BIN: &[u8] = include_bytes!("../../proof/place_bid_v1.zk.bin");
pub const REFUND_BID_V1_BIN: &[u8] = include_bytes!("../../proof/refund_bid_v1.zk.bin");
pub const SETTLE_AUCTION_V1_BIN: &[u8] = include_bytes!("../../proof/settle_auction_v1.zk.bin");
