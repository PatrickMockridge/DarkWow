//! Monero — p2pool-Anchored Finality Gadget
//!
//! Populates Monero anchor data (block height + hash) from p2pool merge-mining
//! params and provides lightweight plausibility verification. Full monerod RPC
//! verification is deferred (Phase 4b).

pub mod rpc;
mod verify;

pub use rpc::{get_block_by_height, get_block_count, MonerodError};
pub use verify::{verify_monero_anchor, MoneroVerifyError};
