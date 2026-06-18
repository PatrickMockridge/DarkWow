//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const ATTEST_SLASH_V1_BIN: &[u8] = include_bytes!("../../proof/attest_slash_v1.zk.bin");
pub const CHECK_NOT_REVOKED_V1_BIN: &[u8] = include_bytes!("../../proof/check_not_revoked_v1.zk.bin");
pub const COMMIT_FEE_SCHEDULE_V1_BIN: &[u8] = include_bytes!("../../proof/commit_fee_schedule_v1.zk.bin");
pub const CONSUME_CLAIM_V1_BIN: &[u8] = include_bytes!("../../proof/consume_claim_v1.zk.bin");
pub const CREATE_ATTESTATION_V1_BIN: &[u8] = include_bytes!("../../proof/create_attestation_v1.zk.bin");
pub const CREATE_CLAIM_V1_BIN: &[u8] = include_bytes!("../../proof/create_claim_v1.zk.bin");
pub const DELEGATE_ATTESTATION_V1_BIN: &[u8] = include_bytes!("../../proof/delegate_attestation_v1.zk.bin");
pub const UPDATE_DELEGATION_V1_BIN: &[u8] = include_bytes!("../../proof/update_delegation_v1.zk.bin");
pub const VERIFY_CHAIN_V1_BIN: &[u8] = include_bytes!("../../proof/verify_chain_v1.zk.bin");
pub const VERIFY_CLAIM_V1_BIN: &[u8] = include_bytes!("../../proof/verify_claim_v1.zk.bin");
