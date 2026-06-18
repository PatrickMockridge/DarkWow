//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const AZT_DEPOSIT_V1_BIN: &[u8] = include_bytes!("../../proof/azt_deposit_v1.zk.bin");
pub const DEPOSIT_V1_BIN: &[u8] = include_bytes!("../../proof/deposit_v1.zk.bin");
pub const LTC_DEPOSIT_V1_BIN: &[u8] = include_bytes!("../../proof/ltc_deposit_v1.zk.bin");
pub const WITHDRAW_V1_BIN: &[u8] = include_bytes!("../../proof/withdraw_v1.zk.bin");
pub const XMR_DEPOSIT_V1_BIN: &[u8] = include_bytes!("../../proof/xmr_deposit_v1.zk.bin");
pub const ZEC_DEPOSIT_V1_BIN: &[u8] = include_bytes!("../../proof/zec_deposit_v1.zk.bin");
