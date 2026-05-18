//! Caribina — Arweave-Anchored Finality Widget
//!
//! Anchors DarkWow block hashes to the Arweave blockchain via ArDrive Turbo
//! for free, providing a finality layer independent of RandomX PoW. Miners
//! post block hash + timestamp + height as an ANS-104 DataItem to ArDrive
//! Turbo's upload service. The returned Arweave transaction ID is embedded
//! in the block header. Nodes verify the anchor by fetching from Arweave
//! gateways. Anchored blocks cannot be reorganized.

pub mod data_item;
pub mod wallet;
pub mod anchor;
pub mod verify;

pub use anchor::anchor_block;
pub use data_item::{DataItem, Tag};
pub use verify::verify_anchor;
pub use wallet::CaribinaWallet;

#[cfg(test)]
mod integration_tests;
