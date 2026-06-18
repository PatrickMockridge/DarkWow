//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const AGGREGATE_V1_BIN: &[u8] = include_bytes!("../../proof/aggregate_v1.zk.bin");
pub const ATTEST_VALUE_V1_BIN: &[u8] = include_bytes!("../../proof/attest_value_v1.zk.bin");
pub const PUSH_VALUE_COMMITMENT_V1_BIN: &[u8] = include_bytes!("../../proof/push_value_commitment_v1.zk.bin");
pub const PUSH_VALUE_V1_BIN: &[u8] = include_bytes!("../../proof/push_value_v1.zk.bin");
pub const REGISTER_ORACLE_V1_BIN: &[u8] = include_bytes!("../../proof/register_oracle_v1.zk.bin");
