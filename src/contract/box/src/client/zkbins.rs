//! ZK circuit binary constants for CLIENT-SIDE proof generation.

pub const PUT_V1_BIN: &[u8] = include_bytes!("../../proof/put_v1.zk.bin");
pub const TAKE_V1_BIN: &[u8] = include_bytes!("../../proof/take_v1.zk.bin");
