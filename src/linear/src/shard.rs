//! Shard-level validation stubs.
//!
//! # ⚠️ POST-MAINNET SCAFFOLDING — DO NOT IMPLEMENT
//!
//! This module is architectural scaffolding for a future scaling phase.
//! Every function body is `todo!()`. The `sharding` feature flag is
//! disabled by default and will remain so until long after mainnet.
//!
//! This is NOT unwired code. It is NOT unfinished pre-mainnet work.
//! See doc/src/arch/consensus/scaling.md for the design.

use crate::block::UncleBlock;
use dwow_sdk::crypto::shard::{CrossShardProof, SettlementBatch, ShardStateRoot};

/// Verify an uncle block acting as a shard state transition.
pub fn verify_shard_block(
    _block: &UncleBlock,
    _expected_state_root: &ShardStateRoot,
) -> Result<bool, crate::Error> {
    todo!("shard block verification (post-mainnet)")
}

/// Verify that a shard's state root exists in the canonical uncle
/// merkle tree at a specific position.
pub fn verify_shard_merkle_inclusion(
    _shard_root: &ShardStateRoot,
    _canonical_uncle_root: &[u8; 32],
    _proof: &[u8; 32],
    _position: u32,
) -> Result<bool, crate::Error> {
    todo!("shard merkle inclusion verification (post-mainnet)")
}

/// Verify a set of cross-shard proofs against the canonical uncle root.
/// See scaling.md §"ZK State Proofs Between Shards".
pub fn verify_cross_shard_proofs(
    _proofs: &[CrossShardProof],
    _canonical_uncle_root: &[u8; 32],
) -> Result<bool, crate::Error> {
    todo!("cross-shard proof batch verification (post-mainnet)")
}

/// Verify a settlement batch — canonical chain validation entry point.
/// See scaling.md §"Inter-Shard Settlement on the Canonical Chain".
pub fn verify_settlement_batch(
    _batch: &SettlementBatch,
    _canonical_uncle_root: &[u8; 32],
) -> Result<bool, crate::Error> {
    todo!("settlement batch verification (post-mainnet)")
}
