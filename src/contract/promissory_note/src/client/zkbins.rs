//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//!
//! These constants are compiled ONLY when `feature = "client"` is enabled
//! (wallet and test targets). They are NOT compiled into WASM builds.
//!
//! WASM builds use their own `include_bytes!` in `entrypoint/mod.rs` inside
//! `init_contract()` — those are local variables for `zkas_db_set()`, used
//! to store circuits in the on-chain database at deploy time. That is a
//! completely separate code path for a different compilation target.
//!
//! This two-location pattern is inherited from upstream. The two `include_bytes!`
//! sites serve different purposes (client proof building vs on-chain circuit
//! registration) and are compiled into mutually exclusive targets.

/// TokenMint_V1 zkas circuit binary
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_TOKEN_MINT_V1_BIN: &[u8] =
/// Mint_V1 zkas circuit binary
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_MINT_V1_BIN: &[u8] =
/// Burn_V1 zkas circuit binary
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_BURN_V1_BIN: &[u8] =
/// BlindOutput_V1 zkas circuit binary (private output coin formation)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_BLIND_OUTPUT_V1_BIN: &[u8] =
/// Redeem_V1 zkas circuit binary (receipt coin formation, value=0)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_REDEEM_V1_BIN: &[u8] =
