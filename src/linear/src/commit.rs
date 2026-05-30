/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Sled batch construction for atomic block commits.
//!
//! Builds sled batches from validated blocks and execution results.
//! The batches are constructed but NOT applied — the caller wraps
//! `apply_batch` calls in a sled `transaction()` for cross-tree atomicity.

use super::{execution::JobResult, Block, PoWConsensus, Result, UncleBlock};

/// Pre-built sled batches for all four trees, ready for an atomic transaction.
pub struct CommitBatch {
    pub blocks: sled::Batch,
    pub uncles: sled::Batch,
    pub contracts: sled::Batch,
    pub consensus: sled::Batch,
}

/// Build sled batches from a validated block, its uncles, and WASM execution
/// results. Pure — constructs batches, does not apply them to the database.
pub fn build_commit_batch(
    block: &Block,
    uncles: &[UncleBlock],
    results: &[JobResult],
    consensus: &PoWConsensus,
) -> CommitBatch {
    let mut blocks_batch = sled::Batch::default();
    let mut uncles_batch = sled::Batch::default();
    let mut contracts_batch = sled::Batch::default();
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

    // Contract state from execution results
    for result in results {
        for (key, value) in &result.state_diff {
            match value {
                Some(v) => contracts_batch.insert(key.as_slice(), v.as_slice()),
                None => contracts_batch.remove(key.as_slice()),
            }
        }
        // Persist newly deployed contracts
        for (contract_id, wasm_bytes) in &result.new_contracts {
            contracts_batch.insert(contract_id.as_slice(), wasm_bytes.as_slice());
        }
    }

    // Consensus state
    consensus.save_to_batch(&mut consensus_batch);

    CommitBatch {
        blocks: blocks_batch,
        uncles: uncles_batch,
        contracts: contracts_batch,
        consensus: consensus_batch,
    }
}
