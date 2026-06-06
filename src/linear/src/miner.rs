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

//! Mining logic for linear blockchain

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use blake3::Hash as Blake3Hash;
use randomx::{RandomXFlags, RandomXVM};
use rand::Rng;

use super::{create_block, Block, PoWConsensus, Transaction};

/// Miner for finding valid PoW blocks using RandomX
pub struct Miner {
    consensus: Arc<PoWConsensus>,
    running: AtomicBool,
}

impl Miner {
    /// Create a new miner with the given consensus
    pub fn new(consensus: Arc<PoWConsensus>) -> Self {
        Self { consensus, running: AtomicBool::new(false) }
    }

    /// Mine a block using RandomX VM
    /// Returns a mined block with valid PoW once found.
    /// If uncles are provided, their merkle root is committed to the block
    /// header (via create_block_with_uncles) before mining begins.
    pub fn mine(
        &self,
        vm: &Arc<RandomXVM>,
        previous: Blake3Hash,
        height: u64,
        txs: Vec<Transaction>,
        target: u32,
        uncles: &[super::UncleBlock],
    ) -> super::Result<Block> {
        self.running.store(true, Ordering::SeqCst);
        let mut rng = rand::thread_rng();
        let atomic_nonce = AtomicU32::new(rng.gen());

        while self.running.load(Ordering::SeqCst) {
            let nonce = atomic_nonce.fetch_add(1, Ordering::SeqCst);

            let mut block = if uncles.is_empty() {
                create_block(previous, height, txs.clone(), target, vm)
            } else {
                crate::create_block_with_uncles(previous, height, txs.clone(), target, uncles, vm)
            };
            block.header.nonce = nonce;
            block.header.randomx_key = Self::derive_key_from_height(height);

            if self.consensus.verify_proof(&block, vm)? {
                return Ok(block)
            }
        }

        Err(super::LinearError::DifficultyNotMet)
    }

    /// Derive RandomX key from block height (key rotation)
    pub fn derive_key_from_height(height: u64) -> [u8; 32] {
        let height_bytes = height.to_le_bytes();
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&height_bytes);
        key
    }

    /// Stop mining
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Create a RandomX VM for mining
    pub fn create_vm(key: &[u8; 32]) -> super::Result<Arc<RandomXVM>> {
        let flags = RandomXFlags::get_recommended_flags();
        let cache = randomx::RandomXCache::new(flags, key)
            .map_err(|e| super::LinearError::RandomXError(format!("Cache: {}", e)))?;
        let vm = RandomXVM::new(flags, Some(cache), None)
            .map_err(|e| super::LinearError::RandomXError(format!("VM: {}", e)))?;
        Ok(Arc::new(vm))
    }
}