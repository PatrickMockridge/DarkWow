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

//! Mempool for linear blockchain
//!
//! Collects transactions with contract calls before they are mined into blocks.

use std::sync::Arc;

use darkfi_linear::Transaction;
use smol::lock::Mutex;

/// Simple mempool for collecting transactions before mining
pub struct Mempool {
    /// Transactions pending inclusion in a block
    txs: Mutex<Vec<Transaction>>,
}

impl Mempool {
    /// Create a new empty mempool
    pub fn new() -> Self {
        Self { txs: Mutex::new(Vec::new()) }
    }

    /// Add a transaction to the mempool
    /// Returns error if transaction is already in mempool
    pub async fn add(&self, tx: Transaction) -> darkfi::Result<()> {
        let mut txs = self.txs.lock().await;
        // Check for duplicates
        let tx_hash = tx.hash();
        for existing in txs.iter() {
            if existing.hash() == tx_hash {
                return Err(darkfi::Error::Custom("Transaction already in mempool".to_string()));
            }
        }
        txs.push(tx);
        Ok(())
    }

    /// Get all transactions and clear the mempool
    pub async fn take_all(&self) -> Vec<Transaction> {
        let mut txs = self.txs.lock().await;
        std::mem::take(&mut *txs)
    }

    /// Get current number of transactions in mempool
    pub async fn len(&self) -> usize {
        self.txs.lock().await.len()
    }

    /// Check if mempool is empty
    pub async fn is_empty(&self) -> bool {
        self.txs.lock().await.is_empty()
    }

    /// Remove a specific transaction by hash
    pub async fn remove(&self, tx_hash: &[u8; 32]) {
        let mut txs = self.txs.lock().await;
        txs.retain(|tx| tx.hash().as_bytes() != tx_hash);
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new()
    }
}

/// Atomic pointer to Mempool
pub type MempoolPtr = Arc<Mempool>;

/// Create a new Mempool
pub fn create_mempool() -> MempoolPtr {
    Arc::new(Mempool::new())
}