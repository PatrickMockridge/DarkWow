//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const INIT_V1_BIN: &[u8] = include_bytes!("../../proof/init_v1.zk.bin");
pub const OPEN_POSITION_V1_BIN: &[u8] = include_bytes!("../../proof/open_position_v1.zk.bin");
pub const ADD_COLLATERAL_V1_BIN: &[u8] = include_bytes!("../../proof/add_collateral_v1.zk.bin");
pub const REMOVE_COLLATERAL_V1_BIN: &[u8] = include_bytes!("../../proof/remove_collateral_v1.zk.bin");
pub const MINT_STABLE_V1_BIN: &[u8] = include_bytes!("../../proof/mint_stable_v1.zk.bin");
pub const REPAY_STABLE_V1_BIN: &[u8] = include_bytes!("../../proof/repay_stable_v1.zk.bin");
pub const LIQUIDATE_V1_BIN: &[u8] = include_bytes!("../../proof/liquidate_v1.zk.bin");
pub const ACCRUE_INTEREST_V1_BIN: &[u8] = include_bytes!("../../proof/accrue_interest_v1.zk.bin");
pub const GOVERNANCE_REPORT_V1_BIN: &[u8] = include_bytes!("../../proof/governance_report_v1.zk.bin");
