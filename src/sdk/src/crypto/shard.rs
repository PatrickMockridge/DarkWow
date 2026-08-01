//! Cross-shard state proofs and settlement.
//!
//! # ⚠️ POST-MAINNET SCAFFOLDING — DO NOT IMPLEMENT
//!
//! This module is architectural scaffolding for a future scaling phase.
//! Every function body is `todo!()`. The `sharding` feature flag is
//! disabled by default and will remain so until long after mainnet.
//!
//! This is NOT unwired code. It is NOT unfinished pre-mainnet work.
//! It exists to document the scaling vision from
//! [scaling.md](../../../doc/src/arch/consensus/scaling.md) in the
//! type system and to reserve the namespace for future implementors.
//!
//! See doc/src/arch/consensus/scaling.md for the design.

use crate::blockchain::BlockHeight;
use crate::crypto::ContractId;
use crate::pasta::pallas;

// ── Shard identity ──────────────────────────────────────────────────

/// Unique identifier for a shard — derived from the uncle miner's region.
/// See scaling.md §"From Uncle Trees to Sharded State".
pub struct ShardId(pub [u8; 32]);

/// Commitment to a shard's full state at a given block height.
/// Stored in the canonical chain's uncle merkle tree.
pub struct ShardStateRoot(pub [u8; 32]);

/// Nullifier preventing double-submission of cross-shard state proofs.
/// Mirrors the O-Cap pattern (commitment → proof → consume) at the shard level.
pub struct ShardNullifier(pub pallas::Base);

// ── Cross-shard proof (O-Cap pattern) ────────────────────────────────

/// Commitment step: proves a shard state root is anchored in the
/// canonical chain's uncle merkle tree at a known height.
pub struct ShardCommitment {
    pub shard_id: ShardId,
    pub state_root: ShardStateRoot,
    pub canonical_height: BlockHeight,
    pub uncle_position: u32,
    pub merkle_path: Vec<[u8; 32]>,
}

/// Full cross-shard O-Cap proof: commitment → ZK proof → nullifier.
/// See scaling.md §"ZK State Proofs Between Shards".
pub struct CrossShardProof {
    pub commitment: ShardCommitment,
    /// Serialized ZK proof that the remote state satisfies the predicate.
    pub zk_proof: Vec<u8>,
    pub nullifier: ShardNullifier,
}

// ── Predicate types ──────────────────────────────────────────────────

/// What the ZK proof proves about the remote shard's state.
/// See scaling.md §"Git-Type State Proof Import".
pub enum PredicateType {
    /// Remote account has at least `amount` balance.
    BalanceGte { account: [u8; 32], amount: u64 },
    /// Remote contract storage slot matches expected value.
    ContractState { contract_id: ContractId, slot: [u8; 32], value: [u8; 32] },
}

// ── Git-style state import ───────────────────────────────────────────

/// Proves that Shard B's state satisfies a predicate, anchored at
/// a known root in the canonical chain's uncle merkle tree.
///
/// The importing shard only needs the proof — the source shard can be
/// offline. See scaling.md §"Git-Type State Proof Import".
pub struct StateImportProof {
    pub source_shard: ShardId,
    pub target_shard: ShardId,
    pub canonical_height: BlockHeight,
    pub uncle_merkle_proof: Vec<[u8; 32]>,
    pub zk_predicate_proof: Vec<u8>,
    pub predicate_type: PredicateType,
    pub predicate_outputs: Vec<pallas::Base>,
    pub imported_state_root: ShardStateRoot,
    pub nullifier: ShardNullifier,
}

/// Verify a git-style state import against the canonical uncle root.
pub fn verify_state_import(
    _proof: &StateImportProof,
    _canonical_uncle_root: &[u8; 32],
) -> Result<bool, crate::error::ContractError> {
    todo!("cross-shard state import verification (post-mainnet)")
}

// ── Aggregate proofs ─────────────────────────────────────────────────

/// Aggregates multiple cross-shard proofs into a single verifiable unit.
/// The canonical chain verifies the aggregate proof without executing
/// the shard transactions. See scaling.md §"Inter-Shard Settlement".
pub struct AggregateProof {
    pub proofs: Vec<CrossShardProof>,
    pub aggregate_state_root: [u8; 32],
    pub aggregate_nullifier_root: [u8; 32],
    pub verification_key_hash: [u8; 32],
}

/// Verify an aggregate proof against the canonical uncle root.
pub fn verify_aggregate_proof(
    _proof: &AggregateProof,
    _canonical_uncle_root: &[u8; 32],
) -> Result<bool, crate::error::ContractError> {
    todo!("aggregate ZK proof verification (post-mainnet)")
}

// ── Settlement batches ───────────────────────────────────────────────

/// A single shard's state transition commitment within a settlement batch.
pub struct ShardStateCommitment {
    pub shard_id: ShardId,
    pub prev_state_root: ShardStateRoot,
    pub new_state_root: ShardStateRoot,
    pub block_height: BlockHeight,
    pub merkle_proof: Vec<[u8; 32]>,
}

/// Transaction referencing state across multiple shards.
pub struct SettlementTransaction {
    pub input_shards: Vec<ShardId>,
    pub state_imports: Vec<StateImportProof>,
    pub output_commitments: Vec<pallas::Base>,
    pub nullifier: ShardNullifier,
}

/// Combined cross-shard settlement posted to the canonical chain.
/// The canonical chain verifies proofs — it does not execute transactions.
/// See scaling.md §"Inter-Shard Settlement on the Canonical Chain".
pub struct SettlementBatch {
    pub batch_id: [u8; 32],
    pub shard_states: Vec<ShardStateCommitment>,
    pub cross_shard_proofs: Vec<CrossShardProof>,
    pub aggregate_proof: Option<AggregateProof>,
    pub transactions: Vec<SettlementTransaction>,
}

/// Verify an entire settlement batch against the canonical uncle root.
pub fn verify_settlement_batch(
    _batch: &SettlementBatch,
    _canonical_uncle_root: &[u8; 32],
) -> Result<bool, crate::error::ContractError> {
    todo!("settlement batch verification (post-mainnet)")
}
