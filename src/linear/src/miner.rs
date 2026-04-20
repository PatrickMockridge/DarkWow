/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Mining logic for linear blockchain

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rand::Rng;

use super::{create_block, Block, PoWConsensus, Transaction};

/// Miner for finding valid PoW blocks
pub struct Miner {
    consensus: Arc<PoWConsensus>,
    running: AtomicBool,
}

impl Miner {
    /// Create a new miner with the given consensus
    pub fn new(consensus: Arc<PoWConsensus>) -> Self {
        Self { consensus, running: AtomicBool::new(false) }
    }

    /// Start mining for a valid block
    pub fn mine(
        &self,
        previous: blake3::Hash,
        height: u64,
        txs: Vec<Transaction>,
        difficulty_target: u32,
    ) -> super::Result<Block> {
        self.running.store(true, Ordering::SeqCst);
        let mut rng = rand::thread_rng();

        while self.running.load(Ordering::SeqCst) {
            let mut block = create_block(previous, height, txs.clone(), difficulty_target);
            block.header.nonce = rng.gen();

            if self.consensus.verify_proof(&block)? {
                return Ok(block)
            }
        }

        Err(super::LinearError::DifficultyNotMet)
    }

    /// Stop mining
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}