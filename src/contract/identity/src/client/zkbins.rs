//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.
//! Consolidated to 3 circuits (HAZOP refactor).

pub const CREATE_CLAIM_BIN: &[u8] = include_bytes!("../../proof/create_claim.zk.bin");
pub const ISSUE_CREDENTIAL_BIN: &[u8] = include_bytes!("../../proof/issue_credential.zk.bin");
pub const VERIFY_CAPABILITY_BIN: &[u8] = include_bytes!("../../proof/verify_capability.zk.bin");
