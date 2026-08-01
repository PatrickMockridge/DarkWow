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

// V1 circuit constants removed (rc3 Batch 4) — V1 .zk source and .zk.bin files deleted.
// V2 circuits: see the corresponding V2 constants below (if any exist).
