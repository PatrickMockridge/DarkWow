//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const CREATE_CLAIM_V1_DAG_BIN: &[u8] = include_bytes!("../../proof/create_claim_dag.zk.bin");
pub const CREATE_CLAIM_V1_L1_BIN: &[u8] = include_bytes!("../../proof/create_claim_l1_sd.zk.bin");
pub const CREATE_CLAIM_V1_L1_V2_BIN: &[u8] = include_bytes!("../../proof/create_claim_l1_sd.zk.bin");
pub const CREATE_CLAIM_V1_MULTI_BIN: &[u8] = include_bytes!("../../proof/create_claim_multi.zk.bin");
pub const CREATE_CLAIM_V1_RATIO_BIN: &[u8] = include_bytes!("../../proof/create_claim_ratio.zk.bin");
