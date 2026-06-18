//! ZK circuit binary embeddings — compiled only when `feature = "client"` is enabled.

pub const DAO_ESCROW_ZKAS_INIT_V1_BIN: &[u8] =
    include_bytes!("../../proof/init_v1.zk.bin");
pub const DAO_ESCROW_ZKAS_PAY_PREMIUM_V1_BIN: &[u8] =
    include_bytes!("../../proof/pay_premium_v1.zk.bin");
pub const DAO_ESCROW_ZKAS_PROPOSE_CLAIM_V1_BIN: &[u8] =
    include_bytes!("../../proof/propose_claim_v1.zk.bin");
pub const DAO_ESCROW_ZKAS_VOTE_CLAIM_V1_BIN: &[u8] =
    include_bytes!("../../proof/vote_claim_v1.zk.bin");
pub const DAO_ESCROW_ZKAS_VERIFY_MEMBER_CAP_V1_BIN: &[u8] =
    include_bytes!("../../proof/verify_member_capability_v1.zk.bin");
pub const DAO_ESCROW_ZKAS_RESOLVE_DISPUTE_V1_BIN: &[u8] =
    include_bytes!("../../proof/resolve_dispute_v1.zk.bin");
