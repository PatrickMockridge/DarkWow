//! ZK circuit binary embeddings — compiled only when `feature = "client"` is enabled.

/// TokenMint_V1 zkas circuit binary
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_TOKEN_MINT_V1_BIN: &[u8] =
    include_bytes!("../../proof/token_mint_v1.zk.bin");
/// Mint_V1 zkas circuit binary
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_MINT_V1_BIN: &[u8] =
    include_bytes!("../../proof/mint_v1.zk.bin");
/// Burn_V1 zkas circuit binary
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_BURN_V1_BIN: &[u8] =
    include_bytes!("../../proof/burn_v1.zk.bin");
/// BlindOutput_V1 zkas circuit binary (private output coin formation)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_BLIND_OUTPUT_V1_BIN: &[u8] =
    include_bytes!("../../proof/blind_output_v1.zk.bin");
/// Redeem_V1 zkas circuit binary (receipt coin formation, value=0)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_REDEEM_V1_BIN: &[u8] =
    include_bytes!("../../proof/redeem_v1.zk.bin");
