//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation of the two-location pattern.

pub const CREATE_ESCROW_V1_BIN: &[u8] =
    include_bytes!("../../proof/create_escrow_v1.zk.bin");
pub const FUND_V1_BIN: &[u8] =
    include_bytes!("../../proof/fund_v1.zk.bin");
pub const CLAIM_V1_BIN: &[u8] =
    include_bytes!("../../proof/claim_v1.zk.bin");
pub const REFUND_V1_BIN: &[u8] =
    include_bytes!("../../proof/refund_v1.zk.bin");
