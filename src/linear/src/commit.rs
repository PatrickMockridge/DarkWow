/* This file is part of DarkWow ... */

//! Sled batch construction and atomic commit for the linear blockchain.

use sled::Transactional;

use super::{Block, PoWConsensus, UncleBlock};

/// Pre-built sled batches for all four trees, ready for an atomic transaction.
pub struct CommitBatch {
    pub blocks: sled::Batch,
    pub uncles: sled::Batch,
    pub contracts: sled::Batch,
    pub consensus: sled::Batch,
}

/// Build sled batches from a validated block, its uncles, an optional
/// contracts overlay batch (from WASM execution), and consensus state.
/// Pure — constructs batches, does not apply them to the database.
pub fn build_commit_batch(
    block: &Block,
    uncles: &[UncleBlock],
    contracts_batch: Option<sled::Batch>,
    consensus: &PoWConsensus,
) -> CommitBatch {
    let mut blocks_batch = sled::Batch::default();
    let mut uncles_batch = sled::Batch::default();
    let mut consensus_batch = sled::Batch::default();

    // Block — keyed by height
    let height_key = block.header.height.to_le_bytes();
    let block_value = serde_json::to_vec(block).unwrap();
    blocks_batch.insert(&height_key, block_value);

    // Uncles — keyed by header hash
    for uncle in uncles {
        let uncle_hash = blake3::hash(&serde_json::to_vec(&uncle.header).unwrap());
        let uncle_value = serde_json::to_vec(uncle).unwrap();
        uncles_batch.insert(uncle_hash.as_bytes(), uncle_value);
    }

    // Consensus state
    consensus.save_to_batch(&mut consensus_batch);

    CommitBatch {
        blocks: blocks_batch,
        uncles: uncles_batch,
        contracts: contracts_batch.unwrap_or_default(),
        consensus: consensus_batch,
    }
}

/// Apply a [`CommitBatch`] atomically to the given sled trees.
/// Uses sled's optimistic-concurrency `transaction()` for cross-tree atomicity.
/// Returns Ok on success, or the sled transaction error on failure.
pub fn commit_atomic(
    blocks_tree: &sled::Tree,
    uncles_tree: &sled::Tree,
    contracts_tree: &sled::Tree,
    consensus_tree: &sled::Tree,
    batch: &CommitBatch,
) -> Result<(), sled::transaction::TransactionError<sled::Error>> {
    (blocks_tree, uncles_tree, contracts_tree, consensus_tree)
        .transaction(|(tx_blocks, tx_uncles, tx_contracts, tx_consensus)| {
            tx_blocks.apply_batch(&batch.blocks)?;
            tx_uncles.apply_batch(&batch.uncles)?;
            tx_contracts.apply_batch(&batch.contracts)?;
            tx_consensus.apply_batch(&batch.consensus)?;
            Ok(())
        })
}
