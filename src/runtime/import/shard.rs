//! Shard host function skeletons.
//!
//! # ⚠️ POST-MAINNET SCAFFOLDING — DO NOT IMPLEMENT
//!
//! This module is architectural scaffolding for a future scaling phase.
//! Every function body is `todo!()`. The `sharding` feature flag is
//! disabled by default and will remain so until long after mainnet.
//!
//! This is NOT unwired code. It is NOT unfinished pre-mainnet work.
//! See doc/src/arch/consensus/scaling.md for the design.

use crate::Result;

/// Host function: verify and record a cross-shard proof.
/// Called by contracts during cross-shard state import.
pub fn merkle_shard_proof_add(
    _env: &super::vm_runtime::Env,
    _data: &[u8],
) -> Result<i64> {
    todo!("shard proof host function (post-mainnet)")
}

/// Host function: verify and accept a settlement batch.
/// Called by the canonical chain during block validation.
pub fn settlement_batch_verify(
    _env: &super::vm_runtime::Env,
    _data: &[u8],
) -> Result<i64> {
    todo!("settlement batch host function (post-mainnet)")
}
