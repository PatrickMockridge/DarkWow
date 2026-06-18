//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//! See native_token zkbins.rs for full explanation.

pub const COMMIT_TICKET_V1_BIN: &[u8] = include_bytes!("../../proof/commit_ticket_v1.zk.bin");
pub const REVEAL_TICKET_V1_BIN: &[u8] = include_bytes!("../../proof/reveal_ticket_v1.zk.bin");
