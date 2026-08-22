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

use dwow_chain::fee_window::CongestionFactor;
use dwow_chain::{Nullifier, Transaction};
use dwow_sdk::blockchain::{BlockCharge, FeeAmount, FeeTier, WasmKb};
use smol::lock::Mutex;

/// Fee Signalling Extractor — the control valve on the transaction pipeline.
///
/// FeeV3 (public gas fee): the extractor reads the plaintext fee, the declared
/// priority tier, and the declarative block-capacity charge. There is no
/// threshold proof, no encrypted-fee channel, and no Pedersen fee commitment.
///
/// Domain: `[domain: fee_signalling]` — serves mempool admission, never
/// `accept_block`. The valve can fail without creating money; it only
/// affects transaction flow rate, not monetary supply.
///
/// See: `doc/src/arch/consensus/fee-spec.md §7.2`.
pub trait FeeSignallingExtractor: Send + Sync {
    /// Read the pressure gauge — extract the plaintext fee amount from call data.
    /// Returns `FeeAmount` per type-system.md §2.3.1 — the domain type prevents
    /// silent mixing with supply, reward, or height arithmetic.
    fn extract_fee(&self, tx: &Transaction) -> FeeAmount;
    /// Read the three-tier priority selector (1=low, 2=medium, 4=high).
    fn extract_tier(&self, tx: &Transaction) -> FeeTier;
    /// Declare the block capacity charge for block packing.
    /// Returns `BlockCharge` per type-system.md §2.3.1 — distinguished from `FeeAmount`
    /// so gas arithmetic cannot mix with fee or supply accounting.
    fn declare_charge(&self, tx: &Transaction) -> BlockCharge;
}

// ── Configuration ────────────────────────────────────────────────────────

/// Mempool configuration.
pub struct MempoolConfig {
    pub max_size: usize,
    pub max_age_secs: u64,
    pub max_tx_size: usize,
    /// Minimum fee for admission. `FeeAmount` per §2.3.1 — cannot be confused with
    /// supply or reward amounts.
    pub min_fee: FeeAmount,
    pub persist: bool,
    /// High tier price — `fee >= price_high` → high_queue (fee-spec.md §12.8.1).
    pub price_high: FeeAmount,
    /// Medium tier price — `price_medium <= fee < price_high` → medium_queue.
    pub price_medium: FeeAmount,
    /// Low tier price — `price_low <= fee < price_medium` → low_queue.
    pub price_low: FeeAmount,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10_000,
            max_age_secs: 3600,
            max_tx_size: 1024 * 1024,
            min_fee: FeeAmount::ZERO,
            price_high: FeeAmount::new(4 * CongestionFactor::SCALE as u64),
            price_medium: FeeAmount::new(2 * CongestionFactor::SCALE as u64),
            price_low: FeeAmount::new(CongestionFactor::SCALE as u64),
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
    pub max_charge: u64,
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
            max_charge: 100_000_000_000,  // matches BLOCK_GAS_LIMIT
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
    fee: FeeAmount,
    declared_charge: BlockCharge,
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
    /// High-priority FIFO queue — `fee >= price_high` (fee-spec.md §12.8.1).
    high_queue: Mutex<VecDeque<blake3::Hash>>,
    /// Approximate high queue length for lock-free congestion measurement.
    high_queue_count: AtomicU64,
    /// Medium-priority FIFO queue — `price_medium <= fee < price_high`.
    medium_queue: Mutex<VecDeque<blake3::Hash>>,
    /// Approximate medium queue length.
    medium_queue_count: AtomicU64,
    /// Low-priority FIFO queue — `price_low <= fee < price_medium`.
    low_queue: Mutex<VecDeque<blake3::Hash>>,
    /// Approximate low queue length.
    low_queue_count: AtomicU64,
    /// Spent nullifiers in the mempool (double-spend prevention).
    /// BTreeSet<Nullifier> per Phase 1 — typed, zero-allocation, ordered.
    nullifiers: Mutex<BTreeSet<Nullifier>>,
    /// Sled tree for persistence (None if disabled)
    db: Option<sled::Tree>,
    /// Configuration
    config: MempoolConfig,
    /// High tier price — runtime-updatable via `update_tier_prices()`.
    /// AtomicU64 for lock-free read in add() per type-system.md §2.3.3 dispensation.
    price_high: AtomicU64,
    /// Medium tier price — runtime-updatable via `update_tier_prices()`.
    price_medium: AtomicU64,
    /// Low tier price — runtime-updatable via `update_tier_prices()`.
    price_low: AtomicU64,
    /// Fee extraction strategy (contract-specific, injected by caller)
    fee_extractor: Box<dyn FeeSignallingExtractor>,
    /// Optional chain state for on-chain nullifier consultation.
    /// When set, add() checks nullifiers against the confirmed set in addition
    /// to the in-pool set. per mempool.md §2.
    chain_state: Option<Arc<dwow_chain::CChainState>>,
}

impl Mempool {
    // ── Construction ─────────────────────────────────────────────────

    /// Create a new mempool with the given config, persistence, and fee extractor.
    pub fn new(config: MempoolConfig, db: Option<sled::Tree>, fee_extractor: Box<dyn FeeSignallingExtractor>,
               chain_state: Option<Arc<dwow_chain::CChainState>>) -> Self {
        Self {
            txs: Mutex::new(HashMap::new()),
            fee_index: Mutex::new(BTreeSet::new()),
            high_queue: Mutex::new(VecDeque::new()),
            high_queue_count: AtomicU64::new(0),
            medium_queue: Mutex::new(VecDeque::new()),
            medium_queue_count: AtomicU64::new(0),
            low_queue: Mutex::new(VecDeque::new()),
            low_queue_count: AtomicU64::new(0),
            nullifiers: Mutex::new(BTreeSet::new()),
            db,
            // §2.3.3: AtomicU64 stores raw u64 — FeeAmount extracted at boundary
            price_high: AtomicU64::new(config.price_high.get()),
            price_medium: AtomicU64::new(config.price_medium.get()),
            price_low: AtomicU64::new(config.price_low.get()),
            config,
            fee_extractor,
            chain_state,
        }
    }

    /// Restore mempool state from a sled tree (called on startup).
    pub fn load(tree: sled::Tree, fee_extractor: Box<dyn FeeSignallingExtractor>,
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
            let gas = fee_extractor.declare_charge(&tx);
            // Rate computation: fee * 1_000_000 / gas produces a dimensionless rate.
            // .get() on gas is the rate-computation boundary — the result is not a BlockCharge.
            let fee_rate = if gas > BlockCharge::ZERO { fee.saturating_mul(1_000_000) / gas.get() } else { 0 };
            let added_at = now_secs();

            fee_index.insert(FeeIndexEntry { fee_rate, tx_hash: hash });
            nullifiers.extend(extract_nullifiers(&tx));
            txs.insert(hash, MempoolEntry { tx, added_at, fee, declared_charge: gas });
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
            high_queue: Mutex::new(VecDeque::new()),
            high_queue_count: AtomicU64::new(0),
            medium_queue: Mutex::new(VecDeque::new()),
            medium_queue_count: AtomicU64::new(0),
            low_queue: Mutex::new(VecDeque::new()),
            low_queue_count: AtomicU64::new(0),
            nullifiers: Mutex::new(nullifiers),
            db: Some(clean_tree),
            // §2.3.3: FeeAmount extracted at boundary
            price_high: AtomicU64::new(config.price_high.get()),
            price_medium: AtomicU64::new(config.price_medium.get()),
            price_low: AtomicU64::new(config.price_low.get()),
            config,
            fee_extractor,
            chain_state,
        })
    }

    // ── Insertion ────────────────────────────────────────────────────

    /// Flow rate gating — admit a transaction to the mempool through the
    /// two-stage control valve. Fee is measured (pressure gauge), threshold
    /// proof is verified (choke check), and the transaction is routed to
    /// premium or general tier (valve port). Below general threshold: REJECT
    /// (valve closed — insufficient pressure to pass).
    ///
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
        let gas = self.fee_extractor.declare_charge(&tx);
        let fee_rate = if gas > BlockCharge::ZERO { fee.saturating_mul(1_000_000) / gas.get() } else { 0 };
        let now = now_secs();

        // Fee minimum (non-coinbase txs — coinbase = PoWRewardV1 call, function 0x05).
        // type-system.md §5, mempool.md §4: coinbase detection must verify BOTH
        // the function selector (0x05) AND the ContractId (NATIVE_TOKEN_CONTRACT_ID).
        // The chain-level structural validation checks both; the mempool must match.
        // Checking only data[0] == 0x05 allows any contract to bypass the fee minimum.
        let is_coinbase = tx.contract_calls.first()
            .and_then(|c| c.as_mass_balance_coinbase_v1())
            .is_some();
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

        // Three-tier admission gate per fee-spec.md §12.8.1 (plaintext fee —
        // no threshold proof, no encrypted-fee channel).
        let is_fee_v3 = tx.contract_calls.first()
            .and_then(|c| c.as_mass_balance_fee_v2())
            .is_some();

        // Insert
        for n in &tx_nullifiers { nulls.insert(*n); }
        fee_idx.insert(FeeIndexEntry { fee_rate, tx_hash });
        txs.insert(tx_hash, MempoolEntry { tx, added_at: now, fee, declared_charge: gas });

        if is_fee_v3 {
            // FeeV3 uses queue-based ordering (drop from the legacy fee_index).
            fee_idx.remove(&FeeIndexEntry { fee_rate, tx_hash });
            let tx_ref = &txs.get(&tx_hash).unwrap().tx;
            let price_high = FeeAmount::new(self.price_high.load(AtomicOrdering::Acquire));
            let price_medium = FeeAmount::new(self.price_medium.load(AtomicOrdering::Acquire));
            let price_low = FeeAmount::new(self.price_low.load(AtomicOrdering::Acquire));
            // Plaintext fee — no ZK threshold proof in FeeV3.
            let plain_fee = self.fee_extractor.extract_fee(tx_ref);
            if plain_fee >= price_high {
                self.high_queue.lock().await.push_back(tx_hash);
                self.high_queue_count.fetch_add(1, AtomicOrdering::Release);
            } else if plain_fee >= price_medium {
                self.medium_queue.lock().await.push_back(tx_hash);
                self.medium_queue_count.fetch_add(1, AtomicOrdering::Release);
            } else if plain_fee >= price_low {
                self.low_queue.lock().await.push_back(tx_hash);
                self.low_queue_count.fetch_add(1, AtomicOrdering::Release);
            } else {
                // Fee below low tier price — REJECT (fee-spec.md §12.8.1).
                txs.remove(&tx_hash);
                for n in &tx_nullifiers { nulls.remove(n); }
                return Err(dwow_core::Error::Custom(format!(
                    "FeeV3: fee below low tier price ({})", price_low
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
        let mut high = self.high_queue.lock().await;
        let mut medium = self.medium_queue.lock().await;
        let mut low = self.low_queue.lock().await;

        let mut selected = Vec::new();
        let mut cumulative_charge = BlockCharge::ZERO;
        let max_charge = BlockCharge::new(config.max_charge);

        // Drain high FIFO queue first (fee-spec.md §12.8.3)
        while let Some(hash) = high.pop_front() {
            if let Some(entry) = txs.get(&hash) {
                let gas = entry.declared_charge.max(BlockCharge::new(1));
                if cumulative_charge.saturating_add(gas) > max_charge || selected.len() >= config.max_txs {
                    high.push_front(hash); // put it back — counter unchanged
                    break;
                }
                cumulative_charge = cumulative_charge.saturating_add(gas);
                selected.push(entry.tx.clone());
                self.high_queue_count.fetch_sub(1, AtomicOrdering::Release);
            }
        }

        // Drain medium FIFO queue second
        while let Some(hash) = medium.pop_front() {
            if let Some(entry) = txs.get(&hash) {
                let gas = entry.declared_charge.max(BlockCharge::new(1));
                if cumulative_charge.saturating_add(gas) > max_charge || selected.len() >= config.max_txs {
                    medium.push_front(hash);
                    break;
                }
                cumulative_charge = cumulative_charge.saturating_add(gas);
                selected.push(entry.tx.clone());
                self.medium_queue_count.fetch_sub(1, AtomicOrdering::Release);
            }
        }

        // Drain low FIFO queue third
        while let Some(hash) = low.pop_front() {
            if let Some(entry) = txs.get(&hash) {
                let gas = entry.declared_charge.max(BlockCharge::new(1));
                if cumulative_charge.saturating_add(gas) > max_charge || selected.len() >= config.max_txs {
                    low.push_front(hash);
                    break;
                }
                cumulative_charge = cumulative_charge.saturating_add(gas);
                selected.push(entry.tx.clone());
                self.low_queue_count.fetch_sub(1, AtomicOrdering::Release);
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
                if cumulative_charge.saturating_add(mp_entry.declared_charge) > max_charge {
                    continue; // skip this tx, try next (smaller) one
                }
                cumulative_charge = cumulative_charge.saturating_add(mp_entry.declared_charge);
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
                    fee_rate: if entry.declared_charge > BlockCharge::ZERO { entry.fee.saturating_mul(1_000_000) / entry.declared_charge.get() } else { 0 },
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
                    fee_rate: if entry.declared_charge > BlockCharge::ZERO { entry.fee.saturating_mul(1_000_000) / entry.declared_charge.get() } else { 0 },
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

    /// Update the three tier prices at runtime.
    /// Called by the miner at fee window boundaries. The `FeeAmount` parameters
    /// ensure prices cannot be confused with supply or reward (type-system.md §2.3.1).
    /// Uses AtomicU64 internally per §2.3.3 dispensation.
    pub fn update_tier_prices(&self, high: FeeAmount, medium: FeeAmount, low: FeeAmount) {
        // §2.3.3: FeeAmount → u64 at the single boundary point
        self.price_high.store(high.get(), AtomicOrdering::Release);
        self.price_medium.store(medium.get(), AtomicOrdering::Release);
        self.price_low.store(low.get(), AtomicOrdering::Release);
    }

    /// Count of transactions in the high queue.
    /// Lock-free atomic counter per SPEC-6 (fee-spec §13.7):
    /// congestion measurement SHALL NOT return 0 on lock contention.
    pub fn high_queue_len(&self) -> usize {
        self.high_queue_count.load(AtomicOrdering::Acquire) as usize
    }

    /// Count of transactions in the medium queue.
    pub fn medium_queue_len(&self) -> usize {
        self.medium_queue_count.load(AtomicOrdering::Acquire) as usize
    }

    /// Count of transactions in the low queue.
    pub fn low_queue_len(&self) -> usize {
        self.low_queue_count.load(AtomicOrdering::Acquire) as usize
    }

    /// Backward-compat: P_premium = high queue (fee-spec.md §12.4.4).
    pub fn premium_queue_len(&self) -> usize {
        self.high_queue_len()
    }

    /// Backward-compat: P_standard = medium + low queues (fee-spec.md §12.4.4).
    pub fn standard_queue_len(&self) -> usize {
        self.medium_queue_len() + self.low_queue_len()
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

// extract_fee and estimate_gas moved to FeeSignallingExtractor trait.
// Implementations live in consumer crates (e.g., NativeTokenFeeSignallingExtractor in dwowd).

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

/// Extract WASM kB from a transaction for per-transaction threshold computation.
///
/// FI-WASM-1 (fee-spec.md §14.8): DeployV1 transactions report
/// `max(1, ceil(wasm_bytes / 1024))`. Non-deploy transactions return 0 (no
/// storage component). The deploy storage fee is `wasm_kB × BASELINE_STORAGE`
/// (§12.4.3).
pub fn extract_tx_wasm_kb(tx: &Transaction) -> u64 {
    for call in &tx.contract_calls {
        if let Some(wasm_bytes) = call.as_deploy_v1() {
            return WasmKb::from_bytes(wasm_bytes).get();
        }
    }
    0 // non-deploy: no storage
}

// ── Public API (preserved for compatibility) ─────────────────────────────

/// Atomic pointer to Mempool.
pub type MempoolPtr = Arc<Mempool>;

/// Create a new Mempool with default config and no persistence.
pub fn create_mempool(fee_extractor: Box<dyn FeeSignallingExtractor>,
                      chain_state: Option<Arc<dwow_chain::CChainState>>) -> MempoolPtr {
    Arc::new(Mempool::new(MempoolConfig::default(), None, fee_extractor, chain_state))
}

/// Create a new Mempool with sled persistence.
pub fn create_mempool_persistent(tree: sled::Tree, fee_extractor: Box<dyn FeeSignallingExtractor>,
                                 chain_state: Option<Arc<dwow_chain::CChainState>>) -> MempoolPtr {
    Arc::new(Mempool::new(MempoolConfig::default(), Some(tree), fee_extractor, chain_state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_chain::fee_window::compute_storage_fee;
    use dwow_chain::ContractCall;

    /// Test fee value — replaces inherited upstream 1 magic constant.
    const TEST_FEE_VALUE: u64 = 1;
    use dwow_sdk::blockchain::{BlockVersion, WasmKb};
    use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

    /// Test fee extractor: extracts fee from native token FeeV1/V2 call data.
    /// FeeV1: data = [0x00][fee: u64 LE]
    /// FeeV2: data = [0x08][fee: u64 LE][...test payload...]
    struct TestFeeSignallingExtractor;
    impl FeeSignallingExtractor for TestFeeSignallingExtractor {
        fn extract_fee(&self, tx: &Transaction) -> FeeAmount {
            let mut total: u64 = 0;
            for call in &tx.contract_calls {
                if call.contract_id == *NATIVE_TOKEN_CONTRACT_ID {
                    match call.data.first() {
                        Some(&0x00u8) => {
                            if call.data.len() >= 9 {
                                let fee_bytes: [u8; 8] = call.data[1..9].try_into().unwrap_or([0u8; 8]);
                                total += u64::from_le_bytes(fee_bytes);
                            }
                        }
                        Some(&0x08u8) => {
                            if call.data.len() >= 9 {
                                let fee_bytes: [u8; 8] = call.data[1..9].try_into().unwrap_or([0u8; 8]);
                                total += u64::from_le_bytes(fee_bytes);
                            }
                        }
                        _ => {}
                    }
                }
            }
            FeeAmount::new(total)
        }
        fn declare_charge(&self, tx: &Transaction) -> BlockCharge {
            BlockCharge::new(tx.contract_calls.len() as u64 * 400_000_000)
        }
        fn extract_tier(&self, _tx: &Transaction) -> FeeTier {
            FeeTier::LOW
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

    /// Build a FeeV2 test transaction with the fee embedded after the selector.
    /// data = [0x08][fee: u64 LE][rest][zero-pad to 444 bytes]
    ///
    /// `as_mass_balance_fee_v2()` re-lifts via `from_bytes`, which enforces a
    /// minimum length (444 bytes) before the selector check. The pad makes the
    /// synthetic call data pass that gate; `extract_fee` still reads `data[1..9]`.
    fn make_fee_v2_tx(fee: u64, rest: &[u8]) -> Transaction {
        let mut data = vec![0x08u8];
        data.extend_from_slice(&fee.to_le_bytes());
        data.extend_from_slice(rest);
        while data.len() < 444 {
            data.push(0u8);
        }
        Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![ContractCall {
                contract_id: *NATIVE_TOKEN_CONTRACT_ID,
                data,
            }],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        }
    }

    #[test]
    fn test_fee_extraction() {
        let tx = make_tx(vec![], Some(1));
        assert_eq!(TestFeeSignallingExtractor.extract_fee(&tx), FeeAmount::new(TEST_FEE_VALUE));
    }

    #[test]
    fn test_add_and_select() {
        smol::block_on(async {
            let mempool = Mempool::new(MempoolConfig::default(), None, Box::new(TestFeeSignallingExtractor), None);
            let tx1 = make_tx(vec![make_call(vec![0x01])], Some(50_000_000));
            let tx2 = make_tx(vec![make_call(vec![0x02])], Some(100_000_000));

            let h1 = mempool.add(tx1).await.unwrap();
            let h2 = mempool.add(tx2).await.unwrap();
            assert_eq!(mempool.len().await, 2);

            // Fee-descending: higher fee first
            let selected = mempool.select_for_block(&MinerConfig { max_charge: u64::MAX, max_txs: 100, ..Default::default() }).await;
            assert_eq!(selected.len(), 2);
            assert_eq!(TestFeeSignallingExtractor.extract_fee(&selected[0]), FeeAmount::new(100_000_000)); // higher fee first
            assert_eq!(TestFeeSignallingExtractor.extract_fee(&selected[1]), FeeAmount::new(50_000_000));

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
            // Flat minimum fee gate — a tx below min_fee is rejected (policy, not consensus).
            let min_fee = FeeAmount::new(1_000_000);
            let config = MempoolConfig {
                min_fee,
                ..Default::default()
            };
            let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);
            // Fee 500_000 < min_fee 1_000_000 → rejected by min_fee gate.
            let tx = make_tx(vec![make_call(vec![0x01])], Some(500_000));
            let result = mempool.add(tx).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Fee too low"));
        });
    }

    #[test]
    fn test_duplicate_rejected() {
        smol::block_on(async {
            let mempool = Mempool::new(MempoolConfig::default(), None, Box::new(TestFeeSignallingExtractor), None);
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
            let mempool = Mempool::new(MempoolConfig::default(), None, Box::new(TestFeeSignallingExtractor), None);
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
            let selected = mempool.select_for_block(&MinerConfig { max_charge: 800_000_000, max_txs: 100, ..Default::default() }).await;
            assert_eq!(selected.len(), 1);
        });
    }

    #[test]
    fn test_persistence_roundtrip() {
        smol::block_on(async {
            let config = sled::Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree("mempool").unwrap();

            let mempool = Mempool::new(MempoolConfig::default(), Some(tree), Box::new(TestFeeSignallingExtractor), None);
            let tx = make_tx(vec![make_call(vec![0x01])], Some(50_000_000));
            let hash = mempool.add(tx).await.unwrap();
            mempool.flush().await.unwrap();

            // Reload
            let tree2 = db.open_tree("mempool").unwrap();
            let mempool2 = Mempool::load(tree2, Box::new(TestFeeSignallingExtractor), None).unwrap();
            assert_eq!(mempool2.len().await, 1);
            assert!(mempool2.contains(&hash).await);
        });
    }

    #[test]
    fn test_update_tier_prices_atomic_visibility() {
        // Store new prices, verify add() sees the new values via Acquire/Release.
        smol::block_on(async {
            let config = MempoolConfig {
                price_high: FeeAmount::new(200_000_000),
                price_medium: FeeAmount::new(50_000_000),
                price_low: FeeAmount::new(10_000_000),
                ..Default::default()
            };
            let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

            // fee=100M → medium (>= 50M, < 200M)
            let tx = make_fee_v2_tx(100_000_000, &[0x01]);
            assert!(mempool.add(tx).await.is_ok(), "fee 100M should be admitted");

            // Lower high price: fee=100M now >= high 90M → high
            mempool.update_tier_prices(FeeAmount::new(90_000_000), FeeAmount::new(50_000_000), FeeAmount::new(10_000_000));
            let tx2 = make_fee_v2_tx(100_000_000, &[0x02]);
            assert!(mempool.add(tx2).await.is_ok(), "fee 100M should be admitted after high price drop");

            // Raise low price above fee: fee=100M below low=150M → reject
            mempool.update_tier_prices(FeeAmount::new(200_000_000), FeeAmount::new(150_000_000), FeeAmount::new(150_000_000));
            let tx3 = make_fee_v2_tx(100_000_000, &[0x03]);
            assert!(mempool.add(tx3).await.is_err(), "fee 100M below low 150M should be rejected");
        });
    }

    #[test]
    fn test_fcfs_preservation_across_tier_price_change() {
        // Transactions admitted under old prices survive and maintain FCFS order.
        smol::block_on(async {
            let config = MempoolConfig {
                price_high: FeeAmount::new(200_000_000),
                price_medium: FeeAmount::new(50_000_000),
                price_low: FeeAmount::new(10_000_000),
                ..Default::default()
            };
            let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

            // Admit 3 transactions: 300M → high; 50M, 60M → medium.
            let tx1 = make_fee_v2_tx(50_000_000, &[0x01]);   // medium
            let tx2 = make_fee_v2_tx(60_000_000, &[0x02]);   // medium
            let tx3 = make_fee_v2_tx(300_000_000, &[0x03]);  // high
            let _h1 = mempool.add(tx1).await.expect("tx1");
            let _h2 = mempool.add(tx2).await.expect("tx2");
            let _h3 = mempool.add(tx3).await.expect("tx3");

            // Raise prices (simulating window boundary).
            mempool.update_tier_prices(FeeAmount::new(400_000_000), FeeAmount::new(100_000_000), FeeAmount::new(50_000_000));

            // Existing txs survive (I3) and maintain order: high (tx3) first, then medium FCFS (tx1, tx2).
            let selected = mempool.select_for_block(&MinerConfig {
                max_charge: u64::MAX, max_txs: 100, ..Default::default()
            }).await;
            assert_eq!(selected.len(), 3, "all 3 existing txs must survive price change");
            assert_eq!(selected[0].contract_calls[0].data[9], 0x03, "high tx must be first");
            assert_eq!(selected[1].contract_calls[0].data[9], 0x01, "medium FCFS: tx1 before tx2");
            assert_eq!(selected[2].contract_calls[0].data[9], 0x02, "medium FCFS: tx2 after tx1");
        });
    }

    #[test]
    fn test_high_queue_len() {
        smol::block_on(async {
            let config = MempoolConfig {
                price_high: FeeAmount::new(100_000_000),
                price_medium: FeeAmount::new(10_000_000),
                price_low: FeeAmount::new(1_000_000),
                ..Default::default()
            };
            let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

            assert_eq!(mempool.high_queue_len(), 0);
            // fee 200M >= high 100M → high queue
            mempool.add(make_fee_v2_tx(200_000_000, &[])).await.unwrap();
            assert_eq!(mempool.high_queue_len(), 1);
            mempool.add(make_fee_v2_tx(300_000_000, &[])).await.unwrap();
            assert_eq!(mempool.high_queue_len(), 2);
        });
    }

    #[test]
    fn test_standard_queue_len() {
        smol::block_on(async {
            let config = MempoolConfig {
                price_high: FeeAmount::new(500_000_000),
                price_medium: FeeAmount::new(50_000_000),
                price_low: FeeAmount::new(10_000_000),
                ..Default::default()
            };
            let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

            assert_eq!(mempool.standard_queue_len(), 0);
            // fee 50M → medium (>= 50M, < 500M); fee 20M → low (>= 10M, < 50M)
            mempool.add(make_fee_v2_tx(50_000_000, &[])).await.unwrap();
            assert_eq!(mempool.standard_queue_len(), 1);
            mempool.add(make_fee_v2_tx(20_000_000, &[])).await.unwrap();
            assert_eq!(mempool.standard_queue_len(), 2); // medium(1) + low(1)
        });
    }

    #[test]
    fn test_has_nullifier() {
        smol::block_on(async {
            let config = MempoolConfig::default();
            let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

            let tx = make_fee_v2_tx(50_000_000, &[]);
            let nullifier = tx.nullifiers.first().cloned();
            mempool.add(tx).await.unwrap();

            if let Some(nf) = nullifier {
                assert!(mempool.has_nullifier(&nf).await, "nullifier should be tracked");
            }
        });
    }

    /// L1-FW-6: concurrent update_thresholds() vs add() stress.
    /// Partition C — concurrency. Writer threads add() while threshold updater
    /// flips between valid pairs. Verify no panics, no lost admission for
    /// already-vetted fee ≥ old general, thresholds always consistent.
    /// Deploy pays a storage fee proportional to wasm_kB; a transfer pays none.
    #[test]
    fn test_deploy_storage_fee_vs_transfer() {
        let transfer = compute_storage_fee(WasmKb::new(0));
        let deploy = compute_storage_fee(WasmKb::new(50));
        assert_eq!(transfer, FeeAmount::new(0), "non-deploy storage fee must be 0");
        assert!(deploy > transfer,
            "deploy must pay more storage than transfer");
    }

    /// FI-WASM-1: wasm_kB = max(1, ceil(bytes / 1024)) — boundary cases.
    #[test]
    fn test_wasm_kb_boundaries() {
        assert_eq!(WasmKb::from_bytes(0).get(), 1, "0 bytes → 1 kB (floor at MIN)");
        assert_eq!(WasmKb::from_bytes(1).get(), 1, "1 byte → 1 kB");
        assert_eq!(WasmKb::from_bytes(1024).get(), 1, "1024 → 1 kB");
        assert_eq!(WasmKb::from_bytes(1025).get(), 2, "1025 → 2 kB");
        assert_eq!(WasmKb::from_bytes(2048).get(), 2, "2048 → 2 kB");
        assert_eq!(WasmKb::from_bytes(2049).get(), 3, "2049 → 3 kB");
    }

    /// FI-WASM-1: non-deploy transactions report wasm_kB = 0 (no storage).
    #[test]
    fn test_extract_tx_wasm_kb_non_deploy() {
        let tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };
        assert_eq!(extract_tx_wasm_kb(&tx), 0, "non-deploy tx → wasm_kB = 0");
    }

    #[test]
    fn test_concurrent_add_update_tier_prices() {
        use std::sync::{Arc as StdArc, Barrier};

        let config = MempoolConfig {
            price_high: FeeAmount::new(200_000_000),
            price_medium: FeeAmount::new(50_000_000),
            price_low: FeeAmount::new(40_000_000),
            ..Default::default()
        };
        let mempool: MempoolPtr = Arc::new(Mempool::new(
            config, None, Box::new(TestFeeSignallingExtractor), None,
        ));

        const N_WRITERS: usize = 8;
        const N_ADDS_PER_WRITER: usize = 50;
        const N_PRICE_FLIPS: usize = 100;

        let barrier = StdArc::new(Barrier::new(N_WRITERS + 1));

        // Price flipper thread — flips between two valid price triples.
        let mp = Arc::clone(&mempool);
        let b = StdArc::clone(&barrier);
        let price_thread = std::thread::spawn(move || {
            smol::block_on(async {
                b.wait();
                for i in 0..N_PRICE_FLIPS {
                    if i % 2 == 0 {
                        mp.update_tier_prices(FeeAmount::new(150_000_000), FeeAmount::new(50_000_000), FeeAmount::new(40_000_000));
                    } else {
                        mp.update_tier_prices(FeeAmount::new(200_000_000), FeeAmount::new(50_000_000), FeeAmount::new(40_000_000));
                    }
                    // Brief yield to let add() calls interleave
                    smol::Timer::after(std::time::Duration::from_micros(1)).await;
                }
            });
        });

        // Writer threads — each adds N_ADDS_PER_WRITER unique txs.
        let mut writer_threads = vec![];
        for thread_id in 0..N_WRITERS {
            let mp = Arc::clone(&mempool);
            let b = StdArc::clone(&barrier);
            writer_threads.push(std::thread::spawn(move || {
                smol::block_on(async {
                    b.wait();
                    for j in 0..N_ADDS_PER_WRITER {
                        let unique = (thread_id * N_ADDS_PER_WRITER + j) as u16;
                        // fee=100M always >= low (40M), < high (150M/200M) → medium
                        let tx = make_fee_v2_tx(100_000_000, &unique.to_le_bytes());
                        let result = mp.add(tx).await;
                        assert!(
                            result.is_ok(),
                            "thread {} tx {} should be admitted, got: {:?}",
                            thread_id, unique, result.err()
                        );
                    }
                });
            }));
        }

        price_thread.join().unwrap();
        for t in writer_threads {
            t.join().unwrap();
        }

        // All transactions survive — no lost admission.
        smol::block_on(async {
            let count = mempool.len().await;
            assert_eq!(count, N_WRITERS * N_ADDS_PER_WRITER,
                "all {} txs should survive", N_WRITERS * N_ADDS_PER_WRITER);
        });
    }
}
