//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const CREATE_CLAIM_V1_BIN: &[u8] = include_bytes!("../../proof/create_claim_v1.zk.bin");
pub const CREATE_CLAIM_V1_DAG_BIN: &[u8] = include_bytes!("../../proof/create_claim_v1_dag.zk.bin");
pub const CREATE_CLAIM_V1_L1_BIN: &[u8] = include_bytes!("../../proof/create_claim_v1_l1.zk.bin");
pub const CREATE_CLAIM_V1_L1_V2_BIN: &[u8] = include_bytes!("../../proof/create_claim_v1_l1_v2.zk.bin");
pub const CREATE_CLAIM_V1_MULTI_BIN: &[u8] = include_bytes!("../../proof/create_claim_v1_multi.zk.bin");
pub const CREATE_CLAIM_V1_RATIO_BIN: &[u8] = include_bytes!("../../proof/create_claim_v1_ratio.zk.bin");
pub const ISSUE_CREDENTIAL_V1_BIN: &[u8] = include_bytes!("../../proof/issue_credential_v1.zk.bin");
pub const VERIFY_CAPABILITY_V1_BIN: &[u8] = include_bytes!("../../proof/verify_capability_v1.zk.bin");
