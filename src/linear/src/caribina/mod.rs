//! Caribina — Arweave-Anchored Finality Widget
//!
//! Anchors DarkWow blocks to the Arweave blockchain via ArDrive Turbo for free, providing a finality
//! layer independent of RandomX PoW. The miner commits a fresh Ed25519 key in the block's **mined
//! region** (`header.anchor_owner`), signs an ANS-104 DataItem binding `anchor_commitment(header)`,
//! carries that proof in the block (`header.caribina_anchor`) and publishes it to ArDrive Turbo.
//!
//! Verification is a **pure local function** of the block — `verify_anchor_proof` checks the
//! signature, that the signer is the committed owner, and that the payload binds this block — so the
//! enforcement sites in `chain_state.rs` need no network and no RandomX VM. Publishing is
//! best-effort and separate: a failed POST costs a block its finality and nothing else.
//!
//! `verify_anchor` (by gateway fetch) and `anchor_block` (build-and-publish in one step) are the
//! older shape, kept for the opt-in live-network conformance tests in `integration_tests.rs`.

pub mod data_item;
pub mod wallet;
pub mod anchor;
pub mod verify;

pub use anchor::{build_anchor_proof, publish_anchor_proof};
pub use data_item::{DataItem, Tag};
pub use verify::{anchor_commitment, verify_anchor, verify_anchor_proof};
pub use wallet::CaribinaWallet;

#[cfg(test)]
mod integration_tests;
