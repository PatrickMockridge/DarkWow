//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const ACCEPT_JOB_V1_BIN: &[u8] = include_bytes!("../../proof/accept_job_v1.zk.bin");
pub const ACCEPT_JOB_WITH_CAPABILITY_V1_BIN: &[u8] = include_bytes!("../../proof/accept_job_with_capability_v1.zk.bin");
pub const CONFIRM_DELIVERY_V1_BIN: &[u8] = include_bytes!("../../proof/confirm_delivery_v1.zk.bin");
pub const CREATE_JOB_V1_BIN: &[u8] = include_bytes!("../../proof/create_job_v1.zk.bin");
pub const DISPUTE_V1_BIN: &[u8] = include_bytes!("../../proof/dispute_v1.zk.bin");
pub const MILESTONE_PAYMENT_V1_BIN: &[u8] = include_bytes!("../../proof/milestone_payment_v1.zk.bin");
pub const REFUND_V1_BIN: &[u8] = include_bytes!("../../proof/refund_v1.zk.bin");
pub const SUBMIT_DELIVERABLE_V1_BIN: &[u8] = include_bytes!("../../proof/submit_deliverable_v1.zk.bin");
pub const SUBMIT_GIT_DELIVERABLE_V1_BIN: &[u8] = include_bytes!("../../proof/submit_git_deliverable_v1.zk.bin");
