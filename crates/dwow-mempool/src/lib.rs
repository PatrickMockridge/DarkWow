/* This file is part of DarkWow
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

//! Production mempool — fee-ordered, persistent, nullifier-dedup.
//!
//! Bitcoin Core CTxMemPool patterns:
//!   - Fee-ordered priority queue (BTreeSet index)
//!   - O(1) lookup by tx hash (HashMap)
//!   - Nullifier dedup at admission (HashSet)
//!   - Size-limit eviction (lowest fee-rate first)
//!   - Sled persistence (survives restart)
//!   - Non-destructive block selection (select_for_block)
//!   - Batch confirmation (mark_mined)
//!
//! HAZOP Gap 1 remediation (2026-07-01).

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dwow_chain::{Nullifier, Transaction};
use smol::lock::Mutex;

/// Strategy for extracting fees and estimating gas from transactions.
/// Implementations encode contract-specific knowledge (e.g., NativeToken FeeV1).
pub trait FeeExtractor: Send + Sync {
    fn extract_fee(&self, tx: &Transaction) -> u64;
    fn estimate_gas(&self, tx: &Transaction) -> u64;

    /// Extract a Pedersen fee commitment from V2 transactions (hidden fee).
    /// REQUIRED — no default. Implementors MUST extract the fee commitment
    /// for FeeV2 transactions.
    fn extract_fee_commitment(&self, tx: &Transaction) -> Option<FeeCommitment>;

    /// Verify the FeeThreshold_V1 proof against a threshold.
    /// REQUIRED — no default. Implementors MUST verify the proof.
    /// Returns true iff the proof is valid for the given threshold.
    fn verify_threshold_proof(&self, tx: &Transaction, threshold: u64) -> bool;
}

/// Pedersen commitment to a fee amount — used when fees are ZK-private.
/// Wraps a pallas::Point. V1 transactions expose clear-text fees instead.
#[derive(Clone, Debug)]
pub struct FeeCommitment(pub dwow_sdk::pasta::pallas::Point);

// ── Configuration ────────────────────────────────────────────────────────

/// Mempool configuration.
pub struct MempoolConfig {
    pub max_size: usize,
    pub max_age_secs: u64,
    pub max_tx_size: usize,
    pub min_fee: u64,
    pub persist: bool,
    /// Premium tier threshold — FeeV2 txs proving fee >= premium_threshold get FIFO priority.
    /// Mandatory. Per fee-spec.md §7.2.
    pub premium_threshold: u64,
    /// General tier threshold — minimum fee for mempool admission.
    /// Transactions with fee below general_threshold are REJECTED. Per fee-spec.md §7.2.
    pub general_threshold: u64,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10_000,
            max_age_secs: 3600,
            max_tx_size: 1024 * 1024,
            min_fee: 0,  // Zero by default — threshold proofs gate admission
            premium_threshold: 42_000_000,
            general_threshold: 1_000_000,
            persist: true,
        }
    }
}

/// Miner fee policy mode.
#[derive(Clone, Debug, PartialEq)]
pub enum MinerMode {
    /// Include all transactions that pass min_fee — default, most inclusive.
    LowFee,
    /// Only include transactions above a configurable fee_rate threshold.
    Medium,
    /// Only include top N% of transactions by fee rate.
    High,
}

/// Miner block assembly configuration.
#[derive(Clone, Debug)]
pub struct MinerConfig {
    pub mode: MinerMode,
    pub max_gas: u64,
    pub max_txs: usize,
    /// Minimum fee per gas for Medium mode (optional).
    pub min_fee_rate: Option<u64>,
    /// Top percentile for High mode (e.g., 0.25 = top 25%). Default 0.25.
    pub top_n_pct: Option<f64>,
}

impl Default for MinerConfig {
    fn default() -> Self {
        Self {
            mode: MinerMode::LowFee,
            max_gas: 100_000_000_000,  // matches BLOCK_GAS_LIMIT
            max_txs: 250,
            min_fee_rate: None,
            top_n_pct: None,
        }
    }
}

// ── Types ────────────────────────────────────────────────────────────────

/// A transaction with metadata for fee ordering and eviction.
#[derive(Clone)]
struct MempoolEntry {
    tx: Transaction,
    added_at: u64,
    fee: u64,
    estimated_gas: u64,
}

/// Fee-index entry for BTreeSet ordering.
/// Ordered by (fee_rate DESC, tx_hash ASC) — higher fee_rate = selected first.
#[derive(Clone, Eq, PartialEq)]
struct FeeIndexEntry {
    fee_rate: u64,
    tx_hash: blake3::Hash,
}

impl Ord for FeeIndexEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other.fee_rate.cmp(&self.fee_rate)
            .then_with(|| self.tx_hash.as_bytes().cmp(other.tx_hash.as_bytes()))
    }
}

impl PartialOrd for FeeIndexEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Production mempool with fee ordering, persistence, and nullifier dedup.
pub struct Mempool {
    /// Transactions indexed by blake3 hash for O(1) lookup
    txs: Mutex<HashMap<blake3::Hash, MempoolEntry>>,
    /// Fee-ordered index for priority block selection (FeeV1, legacy)
    fee_index: Mutex<BTreeSet<FeeIndexEntry>>,
    /// Premium FIFO queue — FeeV2 txs proving fee >= premium_threshold (fee-spec.md §7)
    premium_queue: Mutex<VecDeque<blake3::Hash>>,
    /// General FIFO queue — FeeV2 txs proving fee >= general_threshold (fee-spec.md §7)
    general_queue: Mutex<VecDeque<blake3::Hash>>,
    /// Spent nullifiers in the mempool (double-spend prevention).
    /// BTreeSet<Nullifier> per Phase 1 — typed, zero-allocation, ordered.
    nullifiers: Mutex<BTreeSet<Nullifier>>,
    /// Sled tree for persistence (None if disabled)
    db: Option<sled::Tree>,
    /// Configuration
    config: MempoolConfig,
    /// Premium tier threshold — runtime-updatable via update_thresholds().
    /// Atomic for lock-free read in add(). Initialized from config.premium_threshold.
    premium_threshold: AtomicU64,
    /// General tier threshold — runtime-updatable via update_thresholds().
    /// Atomic for lock-free read in add(). Initialized from config.general_threshold.
    general_threshold: AtomicU64,
    /// Fee extraction strategy (contract-specific, injected by caller)
    fee_extractor: Box<dyn FeeExtractor>,
    /// Optional chain state for on-chain nullifier consultation.
    /// When set, add() checks nullifiers against the confirmed set in addition
    /// to the in-pool set. per mempool.md §2.
    chain_state: Option<Arc<dwow_chain::CChainState>>,
}

impl Mempool {
    // ── Construction ─────────────────────────────────────────────────

    /// Create a new mempool with the given config, persistence, and fee extractor.
    pub fn new(config: MempoolConfig, db: Option<sled::Tree>, fee_extractor: Box<dyn FeeExtractor>,
               chain_state: Option<Arc<dwow_chain::CChainState>>) -> Self {
        Self {
            txs: Mutex::new(HashMap::new()),
            fee_index: Mutex::new(BTreeSet::new()),
            premium_queue: Mutex::new(VecDeque::new()),
            general_queue: Mutex::new(VecDeque::new()),
            nullifiers: Mutex::new(BTreeSet::new()),
            db,
            premium_threshold: AtomicU64::new(config.premium_threshold),
            general_threshold: AtomicU64::new(config.general_threshold),
            config,
            fee_extractor,
            chain_state,
        }
    }

    /// Restore mempool state from a sled tree (called on startup).
    pub fn load(tree: sled::Tree, fee_extractor: Box<dyn FeeExtractor>,
                chain_state: Option<Arc<dwow_chain::CChainState>>) -> dwow_core::Result<Self> {
        let config = MempoolConfig::default();
        let mut txs = HashMap::new();
        let mut fee_index = BTreeSet::new();
        let mut nullifiers = BTreeSet::new();

        for item in tree.iter() {
            let (key, value) = item.map_err(|e| dwow_core::Error::Custom(
                format!("mempool sled iter: {e}")
            ))?;
            let tx: Transaction = serde_json::from_slice(&value)
                .map_err(|e| dwow_core::Error::Custom(
                    format!("mempool deserialize: {e}")
                ))?;
            let hash = tx.hash();
            let fee = fee_extractor.extract_fee(&tx);
            let gas = fee_extractor.estimate_gas(&tx);
            let fee_rate = if gas > 0 { fee.saturating_mul(1_000_000) / gas } else { 0 };
            let added_at = now_secs();

            fee_index.insert(FeeIndexEntry { fee_rate, tx_hash: hash });
            nullifiers.extend(extract_nullifiers(&tx));
            txs.insert(hash, MempoolEntry { tx, added_at, fee, estimated_gas: gas });
            // Remove old key format if present (clean migration)
            let _ = tree.remove(key);
        }

        // Re-persist clean entries
        let clean_tree = tree;
        for (hash, entry) in &txs {
            let value = serde_json::to_vec(&entry.tx)
                .map_err(|e| dwow_core::Error::Custom(format!("mempool serialize: {e}")))?;
            clean_tree.insert(hash.as_bytes(), value)
                .map_err(|e| dwow_core::Error::Custom(format!("mempool sled write: {e}")))?;
        }
        let _ = clean_tree.flush();

        Ok(Self {
            txs: Mutex::new(txs),
            fee_index: Mutex::new(fee_index),
            premium_queue: Mutex::new(VecDeque::new()),
            general_queue: Mutex::new(VecDeque::new()),
            nullifiers: Mutex::new(nullifiers),
            db: Some(clean_tree),
            premium_threshold: AtomicU64::new(config.premium_threshold),
            general_threshold: AtomicU64::new(config.general_threshold),
            config,
            fee_extractor,
            chain_state,
        })
    }

    // ── Insertion ────────────────────────────────────────────────────

    /// Add a transaction to the mempool.
    /// Returns the tx hash on success.
    pub async fn add(&self, tx: Transaction) -> dwow_core::Result<blake3::Hash> {
        let tx_hash = tx.hash();

        // Reject empty transactions
        if tx.contract_calls.is_empty() && tx.inputs.is_empty() {
            return Err(dwow_core::Error::Custom(
                "Transaction has no contract calls and no inputs".to_string()
            ));
        }

        // Size limit
        if let Ok(serialized) = serde_json::to_vec(&tx) {
            if serialized.len() > self.config.max_tx_size {
                return Err(dwow_core::Error::Custom(format!(
                    "Transaction too large: {} bytes (max: {})",
                    serialized.len(), self.config.max_tx_size
                )));
            }
        }

        let fee = self.fee_extractor.extract_fee(&tx);
        let gas = self.fee_extractor.estimate_gas(&tx);
        let fee_rate = if gas > 0 { fee.saturating_mul(1_000_000) / gas } else { 0 };
        let now = now_secs();

        // Fee minimum (non-coinbase txs — coinbase = PoWRewardV1 call, function 0x05).
        // type-system.md §5, mempool.md §4: coinbase detection must verify BOTH
        // the function selector (0x05) AND the ContractId (NATIVE_TOKEN_CONTRACT_ID).
        // The chain-level structural validation checks both; the mempool must match.
        // Checking only data[0] == 0x05 allows any contract to bypass the fee minimum.
        let is_coinbase = tx.contract_calls.first()
            .map_or(false, |c| c.data.first() == Some(&0x05)
                && c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID);
        if !is_coinbase && fee < self.config.min_fee {
            return Err(dwow_core::Error::Custom(format!(
                "Fee too low: {} (minimum: {})", fee, self.config.min_fee
            )));
        }

        // Extract nullifiers for dedup
        let tx_nullifiers = extract_nullifiers(&tx);

        // Defense-in-depth: warn if a spend tx has no nullifiers.
        // The wallet should always populate tx.nullifiers for spend paths.
        if !tx.contract_calls.is_empty() && tx_nullifiers.is_empty() && !is_coinbase {
            tracing::warn!(target: "dwowd::mempool",
                "Transaction {} has contract calls but no nullifiers — wallet should populate tx.nullifiers",
                tx.hash());
        }

        let mut txs = self.txs.lock().await;
        let mut fee_idx = self.fee_index.lock().await;
        let mut nulls = self.nullifiers.lock().await;

        // Evict stale entries
        self.evict_stale_locked(&mut txs, &mut fee_idx, &mut nulls, now);

        // Duplicate check
        if txs.contains_key(&tx_hash) {
            return Err(dwow_core::Error::Custom(
                "Transaction already in mempool".to_string()
            ));
        }

        // Nullifier dedup (in-mempool)
        for n in &tx_nullifiers {
            if nulls.contains(n) {
                return Err(dwow_core::Error::Custom(
                    "Double-spend: nullifier already in mempool".to_string()
                ));
            }
        }

        // On-chain nullifier check (mempool.md §2):
        // "Admission SHALL consult the confirmed nullifier set, not only the in-pool set."
        if let Some(ref cs) = self.chain_state {
            for n in &tx_nullifiers {
                if cs.has_nullifier(n) {
                    return Err(dwow_core::Error::Custom(format!(
                        "Double-spend: nullifier already confirmed on-chain"
                    )));
                }
            }
        }

        // Size limit with eviction
        if txs.len() >= self.config.max_size {
            // Evict the single lowest-fee-rate transaction
            if let Some(lowest) = fee_idx.last().cloned() {
                if lowest.fee_rate < fee_rate || txs.len() >= self.config.max_size {
                    if let Some(evicted) = txs.remove(&lowest.tx_hash) {
                        fee_idx.remove(&lowest);
                        let evict_nulls = extract_nullifiers(&evicted.tx);
                        for n in &evict_nulls { nulls.remove(n); }
                    }
                }
            }
            // Still full after eviction attempt
            if txs.len() >= self.config.max_size {
                return Err(dwow_core::Error::Custom("Mempool is full".to_string()));
            }
        }

        // Two-tier admission gate per fee-spec.md §7.2.
        // Check BEFORE insertion to avoid borrow-after-move.
        let is_fee_v2 = tx.contract_calls.first().map_or(false, |c| {
            c.data.first() == Some(&0x08)
                && c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
        });

        // Insert
        for n in &tx_nullifiers { nulls.insert(*n); }
        fee_idx.insert(FeeIndexEntry { fee_rate, tx_hash });
        txs.insert(tx_hash, MempoolEntry { tx, added_at: now, fee, estimated_gas: gas });

        if is_fee_v2 {
            // Remove from legacy fee_index — FeeV2 uses queue-based ordering
            fee_idx.remove(&FeeIndexEntry { fee_rate, tx_hash });
            let premium = self.premium_threshold.load(AtomicOrdering::Acquire);
            let general = self.general_threshold.load(AtomicOrdering::Acquire);
            if self.fee_extractor.verify_threshold_proof(
                &txs.get(&tx_hash).unwrap().tx, premium,
            ) {
                self.premium_queue.lock().await.push_back(tx_hash);
            } else if self.fee_extractor.verify_threshold_proof(
                &txs.get(&tx_hash).unwrap().tx, general,
            ) {
                self.general_queue.lock().await.push_back(tx_hash);
            } else {
                // Fee below general threshold — REJECT (fee-spec.md §7.2).
                // Roll back insertion.
                txs.remove(&tx_hash);
                for n in &tx_nullifiers { nulls.remove(n); }
                return Err(dwow_core::Error::Custom(format!(
                    "FeeV2: fee below general threshold ({})", general
                )));
            }
        }

        // Persist
        if let Some(ref db) = self.db {
            if let Some(entry) = txs.get(&tx_hash) {
                if let Ok(value) = serde_json::to_vec(&entry.tx) {
                    if let Err(e) = db.insert(tx_hash.as_bytes(), value) {
                        tracing::warn!(target: "dwowd::mempool",
                            "sled insert failed during add: {}", e);
                    }
                }
            }
        }

        Ok(tx_hash)
    }

    // ── Block Selection ──────────────────────────────────────────────

    /// Select transactions for block inclusion, fee-descending order.
    /// Uses MinerConfig to determine fee filtering policy:
    ///   LowFee (default): all tx that pass min_fee
    ///   Medium: only tx with fee_rate >= min_fee_rate
    ///   High: only top N% by fee_rate
    /// Does NOT remove from mempool — call `mark_mined` after block acceptance.
    pub async fn select_for_block(&self, config: &MinerConfig) -> Vec<Transaction> {
        let txs = self.txs.lock().await;
        let fee_idx = self.fee_index.lock().await;
        let mut premium = self.premium_queue.lock().await;
        let mut general = self.general_queue.lock().await;

        let mut selected = Vec::new();
        let mut cumulative_gas: u64 = 0;

        // Drain premium FIFO queue first (fee-spec.md §7.3)
        while let Some(hash) = premium.pop_front() {
            if let Some(entry) = txs.get(&hash) {
                let gas = entry.estimated_gas.max(1);
                if cumulative_gas + gas > config.max_gas || selected.len() >= config.max_txs {
                    premium.push_front(hash); // put it back
                    break;
                }
                cumulative_gas += gas;
                selected.push(entry.tx.clone());
            }
        }

        // Drain general FIFO queue second
        if cumulative_gas < config.max_gas && selected.len() < config.max_txs {
            while let Some(hash) = general.pop_front() {
                if let Some(entry) = txs.get(&hash) {
                    let gas = entry.estimated_gas.max(1);
                    if cumulative_gas + gas > config.max_gas || selected.len() >= config.max_txs {
                        general.push_front(hash);
                        break;
                    }
                    cumulative_gas += gas;
                    selected.push(entry.tx.clone());
                }
            }
        }

        // Legacy fee_index for remaining capacity (FeeV1 txs)

        // Compute fee cutoff for High mode
        let fee_cutoff: Option<u64> = match config.mode {
            MinerMode::High => {
                let pct = config.top_n_pct.unwrap_or(0.25);
                let count = fee_idx.len();
                let top_n = ((count as f64) * pct).ceil() as usize;
                let top_n = top_n.max(1);
                fee_idx.iter().nth(top_n.saturating_sub(1)).map(|e| e.fee_rate)
            }
            MinerMode::Medium => config.min_fee_rate,
            MinerMode::LowFee => None,
        };

        for entry in fee_idx.iter() {
            if selected.len() >= config.max_txs {
                break;
            }
            // Apply fee filtering
            if let Some(cutoff) = fee_cutoff {
                if entry.fee_rate < cutoff {
                    continue;
                }
            }
            if let Some(mp_entry) = txs.get(&entry.tx_hash) {
                if cumulative_gas + mp_entry.estimated_gas > config.max_gas {
                    continue; // skip this tx, try next (smaller) one
                }
                cumulative_gas += mp_entry.estimated_gas;
                selected.push(mp_entry.tx.clone());
            }
        }

        selected
    }

    /// Remove confirmed transactions from the mempool.
    pub async fn mark_mined(&self, tx_hashes: &[blake3::Hash]) {
        let mut txs = self.txs.lock().await;
        let mut fee_idx = self.fee_index.lock().await;
        let mut nulls = self.nullifiers.lock().await;

        for hash in tx_hashes {
            if let Some(entry) = txs.remove(hash) {
                fee_idx.remove(&FeeIndexEntry {
                    fee_rate: if entry.estimated_gas > 0 { entry.fee.saturating_mul(1_000_000) / entry.estimated_gas } else { 0 },
                    tx_hash: *hash,
                });
                let entry_nulls = extract_nullifiers(&entry.tx);
                for n in &entry_nulls { nulls.remove(n); }
                // Remove from sled
                if let Some(ref db) = self.db {
                    if let Err(e) = db.remove(hash.as_bytes()) {
                        tracing::warn!(target: "dwowd::mempool",
                            "sled remove failed during mark_mined: {}", e);
                    }
                }
            }
        }
    }

    // ── Maintenance ──────────────────────────────────────────────────

    /// Remove expired entries.
    pub async fn evict_stale(&self) -> usize {
        let now = now_secs();
        let mut txs = self.txs.lock().await;
        let mut fee_idx = self.fee_index.lock().await;
        let mut nulls = self.nullifiers.lock().await;
        self.evict_stale_locked(&mut txs, &mut fee_idx, &mut nulls, now)
    }

    fn evict_stale_locked(
        &self, txs: &mut HashMap<blake3::Hash, MempoolEntry>,
        fee_idx: &mut BTreeSet<FeeIndexEntry>, nulls: &mut BTreeSet<Nullifier>, now: u64,
    ) -> usize {
        let max_age = self.config.max_age_secs;
        let stale: Vec<blake3::Hash> = txs.iter()
            .filter(|(_, e)| now.saturating_sub(e.added_at) >= max_age)
            .map(|(h, _)| *h)
            .collect();

        for hash in &stale {
            if let Some(entry) = txs.remove(hash) {
                fee_idx.remove(&FeeIndexEntry {
                    fee_rate: if entry.estimated_gas > 0 { entry.fee.saturating_mul(1_000_000) / entry.estimated_gas } else { 0 },
                    tx_hash: *hash,
                });
                let entry_nulls = extract_nullifiers(&entry.tx);
                for n in &entry_nulls { nulls.remove(n); }
                if let Some(ref db) = self.db {
                    if let Err(e) = db.remove(hash.as_bytes()) {
                        tracing::warn!(target: "dwowd::mempool",
                            "sled remove failed during eviction: {}", e);
                    }
                }
            }
        }
        stale.len()
    }

    // ── Queries ──────────────────────────────────────────────────────

    pub async fn len(&self) -> usize {
        self.txs.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.txs.lock().await.is_empty()
    }

    pub async fn contains(&self, hash: &blake3::Hash) -> bool {
        self.txs.lock().await.contains_key(hash)
    }

    /// Check whether a nullifier is already in the mempool (double-spend prevention).
    /// Used by the wallet for the optimistic read: capabilities whose nullifiers
    /// are pending in the mempool are not spendable until confirmed or dropped.
    pub async fn has_nullifier(&self, nullifier: &dwow_chain::Nullifier) -> bool {
        self.nullifiers.lock().await.contains(nullifier)
    }

    /// Update premium and general fee thresholds at runtime.
    /// Called by the miner at fee window boundaries. Uses AtomicU64 for
    /// lock-free writes — no &mut self needed, compatible with prepare_block().
    pub fn update_thresholds(&self, premium: u64, general: u64) {
        self.premium_threshold.store(premium, AtomicOrdering::Release);
        self.general_threshold.store(general, AtomicOrdering::Release);
    }

    /// Current premium threshold (memory-fenced read).
    pub fn premium_threshold(&self) -> u64 {
        self.premium_threshold.load(AtomicOrdering::Acquire)
    }

    /// Current general threshold (memory-fenced read).
    pub fn general_threshold(&self) -> u64 {
        self.general_threshold.load(AtomicOrdering::Acquire)
    }

    /// Remove a specific transaction by hash.
    pub async fn remove(&self, tx_hash: &[u8; 32]) {
        let hash = blake3::Hash::from_bytes(*tx_hash);
        self.mark_mined(&[hash]).await;
    }

    // ── Persistence ──────────────────────────────────────────────────

    /// Flush all entries to sled and fsync.
    pub async fn flush(&self) -> dwow_core::Result<()> {
        if let Some(ref db) = self.db {
            let txs = self.txs.lock().await;
            for (hash, entry) in txs.iter() {
                let value = serde_json::to_vec(&entry.tx)
                    .map_err(|e| dwow_core::Error::Custom(format!("mempool serialize: {e}")))?;
                db.insert(hash.as_bytes(), value)
                    .map_err(|e| dwow_core::Error::Custom(format!("mempool sled write: {e}")))?;
            }
            db.flush()
                .map_err(|e| dwow_core::Error::Custom(format!("mempool sled flush: {e}")))?;
        }
        Ok(())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// extract_fee and estimate_gas moved to FeeExtractor trait.
// Implementations live in consumer crates (e.g., NativeTokenFeeExtractor in dwowd).

/// Extract pre-computed nullifiers from a transaction.
/// Extract typed nullifiers from a transaction for mempool indexing.
/// These are set by the wallet during transaction construction —
/// the mempool reads them directly. No parsing needed.
fn extract_nullifiers(tx: &Transaction) -> Vec<Nullifier> {
    tx.nullifiers.iter()
        .filter(|n| !n.is_zero())
        .copied()
        .collect()
}

// ── Public API (preserved for compatibility) ─────────────────────────────

/// Atomic pointer to Mempool.
pub type MempoolPtr = Arc<Mempool>;

/// Create a new Mempool with default config and no persistence.
pub fn create_mempool(fee_extractor: Box<dyn FeeExtractor>,
                      chain_state: Option<Arc<dwow_chain::CChainState>>) -> MempoolPtr {
    Arc::new(Mempool::new(MempoolConfig::default(), None, fee_extractor, chain_state))
}

/// Create a new Mempool with sled persistence.
pub fn create_mempool_persistent(tree: sled::Tree, fee_extractor: Box<dyn FeeExtractor>,
                                 chain_state: Option<Arc<dwow_chain::CChainState>>) -> MempoolPtr {
    Arc::new(Mempool::new(MempoolConfig::default(), Some(tree), fee_extractor, chain_state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_chain::ContractCall;
    use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

    /// Test fee extractor: returns 42_000_000 per call to native token FeeV1.
    struct TestFeeExtractor;
    impl FeeExtractor for TestFeeExtractor {
        fn extract_fee(&self, tx: &Transaction) -> u64 {
            let mut total: u64 = 0;
            for call in &tx.contract_calls {
                if call.contract_id == *NATIVE_TOKEN_CONTRACT_ID && call.data.first() == Some(&0x00u8) {
                    if call.data.len() >= 9 {
                        let fee_bytes: [u8; 8] = call.data[1..9].try_into().unwrap_or([0u8; 8]);
                        total += u64::from_le_bytes(fee_bytes);
                    }
                }
            }
            total
        }
        fn estimate_gas(&self, tx: &Transaction) -> u64 {
            tx.contract_calls.len() as u64 * 400_000_000
        }
        fn extract_fee_commitment(&self, _tx: &Transaction) -> Option<FeeCommitment> {
            None
        }
        fn verify_threshold_proof(&self, _tx: &Transaction, _threshold: u64) -> bool {
            false
        }
    }

    fn make_tx(calls: Vec<ContractCall>, fee: Option<u64>) -> Transaction {
        let mut tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: calls,
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };
        if let Some(f) = fee {
            // Add a mock FeeV1 call to set the fee
            let mut fee_data = vec![0x00u8]; // FeeV1 function code
            fee_data.extend_from_slice(&f.to_le_bytes());
            tx.contract_calls.push(ContractCall {
                contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                data: fee_data,
            });
        }
        tx
    }

    fn make_call(data: Vec<u8>) -> ContractCall {
        ContractCall {
            contract_id: dwow_sdk::crypto::ContractId::from_bytes([1u8; 32]).unwrap(),
            data,
        }
    }

    #[test]
    fn test_fee_extraction() {
        let tx = make_tx(vec![], Some(42_000_000));
        assert_eq!(TestFeeExtractor.extract_fee(&tx), 42_000_000);
    }

    #[test]
    fn test_add_and_select() {
        smol::block_on(async {
            let mempool = Mempool::new(MempoolConfig::default(), None, Box::new(TestFeeExtractor), None);
            let tx1 = make_tx(vec![make_call(vec![0x01])], Some(50_000_000));
            let tx2 = make_tx(vec![make_call(vec![0x02])], Some(100_000_000));

            let h1 = mempool.add(tx1).await.unwrap();
            let h2 = mempool.add(tx2).await.unwrap();
            assert_eq!(mempool.len().await, 2);

            // Fee-descending: higher fee first
            let selected = mempool.select_for_block(&MinerConfig { max_gas: u64::MAX, max_txs: 100, ..Default::default() }).await;
            assert_eq!(selected.len(), 2);
            assert_eq!(TestFeeExtractor.extract_fee(&selected[0]), 100_000_000); // higher fee first
            assert_eq!(TestFeeExtractor.extract_fee(&selected[1]), 50_000_000);

            // Still in mempool after select
            assert_eq!(mempool.len().await, 2);

            // Mark mined
            mempool.mark_mined(&[h1]).await;
            assert_eq!(mempool.len().await, 1);
            assert!(mempool.contains(&h2).await);
        });
    }

    #[test]
    fn test_fee_too_low_rejected() {
        smol::block_on(async {
            let mempool = Mempool::new(MempoolConfig::default(), None, Box::new(TestFeeExtractor), None);
            let tx = make_tx(vec![make_call(vec![0x01])], Some(1_000_000)); // below 42M min
            let result = mempool.add(tx).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Fee too low"));
        });
    }

    #[test]
    fn test_duplicate_rejected() {
        smol::block_on(async {
            let mempool = Mempool::new(MempoolConfig::default(), None, Box::new(TestFeeExtractor), None);
            let tx = make_tx(vec![make_call(vec![0x01])], Some(50_000_000));
            mempool.add(tx.clone()).await.unwrap();
            let result = mempool.add(tx).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("already in mempool"));
        });
    }

    #[test]
    fn test_gas_limit_respected() {
        smol::block_on(async {
            let mempool = Mempool::new(MempoolConfig::default(), None, Box::new(TestFeeExtractor), None);
            // Each call = 400M gas. 1 payload call + FeeV1 = 2 calls = 800M per tx.
            // Gas limit = 800M — should only fit one tx.
            let tx1 = make_tx(
                vec![make_call(vec![1])],
                Some(50_000_000),
            );
            let tx2 = make_tx(
                vec![make_call(vec![2])],
                Some(100_000_000),
            );
            mempool.add(tx1).await.unwrap();
            mempool.add(tx2).await.unwrap();

            // Gas limit 800M — should only fit one tx (2 calls × 400M = 800M)
            let selected = mempool.select_for_block(&MinerConfig { max_gas: 800_000_000, max_txs: 100, ..Default::default() }).await;
            assert_eq!(selected.len(), 1);
        });
    }

    #[test]
    fn test_persistence_roundtrip() {
        smol::block_on(async {
            let config = sled::Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree("mempool").unwrap();

            let mempool = Mempool::new(MempoolConfig::default(), Some(tree), Box::new(TestFeeExtractor), None);
            let tx = make_tx(vec![make_call(vec![0x01])], Some(50_000_000));
            let hash = mempool.add(tx).await.unwrap();
            mempool.flush().await.unwrap();

            // Reload
            let tree2 = db.open_tree("mempool").unwrap();
            let mempool2 = Mempool::load(tree2, Box::new(TestFeeExtractor), None).unwrap();
            assert_eq!(mempool2.len().await, 1);
            assert!(mempool2.contains(&hash).await);
        });
    }
}
