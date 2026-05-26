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

//! Mempool for linear blockchain
//!
//! Collects transactions with contract calls before they are mined into blocks.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dwow_linear::Transaction;
use smol::lock::Mutex;

/// Maximum number of transactions allowed in the mempool
const MAX_MEMPOOL_SIZE: usize = 10_000;

/// Maximum age of a transaction in the mempool before eviction (1 hour)
const MAX_MEMPOOL_AGE_SECS: u64 = 3600;

/// A transaction with its admission timestamp
struct MempoolEntry {
    tx: Transaction,
    added_at: u64,
}

/// Simple mempool for collecting transactions before mining
pub struct Mempool {
    /// Transactions pending inclusion in a block
    txs: Mutex<Vec<MempoolEntry>>,
}

impl Mempool {
    /// Create a new empty mempool
    pub fn new() -> Self {
        Self { txs: Mutex::new(Vec::new()) }
    }

    /// Add a transaction to the mempool.
    /// Returns error if mempool is full or transaction is already in mempool.
    pub async fn add(&self, tx: Transaction) -> dwow::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut txs = self.txs.lock().await;

        // Evict stale transactions
        txs.retain(|e| now.saturating_sub(e.added_at) < MAX_MEMPOOL_AGE_SECS);

        // Check for duplicates
        let tx_hash = tx.hash();
        for existing in txs.iter() {
            if existing.tx.hash() == tx_hash {
                return Err(dwow::Error::Custom("Transaction already in mempool".to_string()));
            }
        }

        // Reject if mempool is full
        if txs.len() >= MAX_MEMPOOL_SIZE {
            return Err(dwow::Error::Custom("Mempool is full".to_string()));
        }

        txs.push(MempoolEntry { tx, added_at: now });
        Ok(())
    }

    /// Get all transactions and clear the mempool
    pub async fn take_all(&self) -> Vec<Transaction> {
        let mut txs = self.txs.lock().await;
        std::mem::take(&mut *txs).into_iter().map(|e| e.tx).collect()
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
        txs.retain(|e| e.tx.hash().as_bytes() != tx_hash);
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