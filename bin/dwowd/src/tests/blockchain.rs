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

//! HeavyweightPipeline + HeavyweightBlock — Modular multi-contract block builder.
//!
//! Replaces the single-contract `HeavyweightPipeline<H>` with a two-layer
//! architecture:
//!
//! - `HeavyweightPipeline` — shared chain state with cached ZK coinbase keys.
//!   Created once per test. Multiple harnesses share one chain.
//! - `HeavyweightBlock` — fluent per-block builder. Created by
//!   `chain.block()`. Accepts contract calls from any harness.
//!
//! ## Usage
//!
//! ```ignore
//! let chain = HeavyweightPipeline::new().await?;
//! chain.init_genesis().await?;
//! let h = DexHarness::spawn();
//! let cid = chain.deploy(&h, "dex", DEX_WASM).await?;
//! let result = h.create_swap(...)?;
//! chain.block()?
//!     .with_call(cid, &h, &result.call_data, vec![result.proof])?
//!     .submit().await?;
//! ```

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use dwow_chain::{CChainState, FinalityConfig, PoWConfig};
use dwow_core::zk::Proof;
use dwow_core::Result;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget};
use dwow_sdk::crypto::{ContractId, NATIVE_TOKEN_CONTRACT_ID};
use dwow_contract_test_harness::harness::ContractHarness;

/// Result of building a coinbase for the block being assembled.
pub struct CoinbaseResult {
    pub tx: dwow_chain::Transaction,
    pub recipient: crate::accounts::MiningRecipient,
    pub coin_value: u64,
    pub coin_commitment: dwow_chain::CoinCommitment,
    pub nullifier: dwow_chain::Nullifier,
    pub coin_blind: dwow_sdk::pasta::pallas::Base,
}

// ── HeavyweightPipeline ───────────────────────────────────────────────────────────

/// Shared chain state for multi-harness tests.
///
/// Owns the sled DB, CChainState, and cached ZK proving keys for coinbase
/// construction. Multiple harnesses share one HeavyweightPipeline.
pub struct HeavyweightPipeline {
    /// Temp sled database
    pub db: Arc<sled::Db>,
    /// Single authoritative chain state
    pub chain_state: Arc<CChainState>,
    /// Cached ZK proving keys for coinbase construction
    pub linear_zk: Arc<crate::registry::model::LinearPowRewardZk>,
    /// Path to temp keys.toml for deterministic test mining keys
    keys_path: std::path::PathBuf,
    /// Strict ZK enforcement — when true, with_call() requires non-empty
    /// proofs for ZK contracts. Default: true.
    pub strict_zk: bool,
}

impl HeavyweightPipeline {
    /// Deterministic test mining key (secret = 0x01...)
    const TEST_KEY_TOML: &'static str =
        "[node0]\nwallet_secret = \
         \"0100000000000000000000000000000000000000000000000000000000000000\"\n";

    /// Create a new HeavyweightPipeline with an empty contracts tree.
    ///
    /// Creates a temp sled DB + CChainState, compiles ZK proving keys for
    /// coinbase construction, and writes a temp keys.toml. After construction,
    /// call `init_genesis()` to create the genesis block.
    pub async fn new() -> Result<Self> {
        static GEN_COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = GEN_COUNTER.fetch_add(1, Ordering::Relaxed);
        let db_dir = std::env::temp_dir()
            .join(format!("dwow_bc_test_{}_{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&db_dir);
        let db = sled::Config::new()
            .path(&db_dir)
            .open()
            .map_err(|e| dwow_core::Error::Custom(format!("sled open: {}", e)))?;
        let db = Arc::new(db);

        let pow_config = PoWConfig {
            target_block_time: 120,
            initial_target: BlockTarget::MAX,
            min_target: BlockTarget::new(1),
            max_target: BlockTarget::MAX,
        };
        let finality_config = FinalityConfig::default();
        let chain_state = CChainState::new(
            db.clone(),
            pow_config.target_block_time,
            pow_config.initial_target,
            pow_config.min_target,
            pow_config.max_target,
            finality_config,
        )
        .map_err(|e| dwow_core::Error::Custom(e.to_string()))?;

        let linear_zk = crate::registry::model::LinearPowRewardZk::new(chain_state.clone())
            .await
            .map_err(|e| dwow_core::Error::Custom(format!("LinearPowRewardZk: {}", e)))?;
        let linear_zk = Arc::new(linear_zk);

        let keys_path = std::env::temp_dir()
            .join(format!("dwow_bc_keys_{}_{}.toml", std::process::id(), n));
        std::fs::write(&keys_path, Self::TEST_KEY_TOML)
            .map_err(|e| dwow_core::Error::Custom(format!("write test keys: {}", e)))?;

        Ok(Self { db, chain_state, linear_zk, keys_path, strict_zk: true })
    }

    /// Initialize the genesis block (height 1).
    ///
    /// Creates the genesis block with 1 coinbase + 9 contract deployments.
    /// Must be called once before any block building.
    pub async fn init_genesis(&self) -> Result<()> {
        let mgr = crate::accounts::AccountManager::open(
            &self.keys_path,
            dwow_sdk::crypto::keypair::Network::Testnet,
            "node0",
        ).map_err(|e| dwow_core::Error::Custom(format!("open test keys: {}", e)))?;
        let recipient = crate::accounts::MiningRecipient::from_account(&mgr, BlockHeight::new(1))
            .map_err(|e| dwow_core::Error::Custom(format!("MiningRecipient: {}", e)))?;
        drop(mgr);
        let magic_bytes = [0xDA, 0x57, 0x01, 0x57];
        crate::init_genesis(&self.chain_state, recipient, magic_bytes).await.map(|_| ())
    }

    /// Deploy a contract WASM into the chain state.
    ///
    /// Stores WASM in the contracts sled tree and runs `__initialize` via
    /// the WASM runtime. Verifies ZK coverage before deployment.
    pub async fn deploy(
        &self,
        harness: &dyn ContractHarness,
        name: &str,
        wasm: &[u8],
    ) -> Result<ContractId> {
        harness.verify_zk_coverage()?;
        let contract_id = derive_contract_id_from_name(name);
        let contracts_tree = self.chain_state.store.contracts_tree().clone();
        let mut overlay = sled_overlay::SledTreeOverlay::new(&contracts_tree);
        overlay.state.cache.insert(
            sled::IVec::from(contract_id.to_bytes().as_slice()),
            sled::IVec::from(wasm),
        );

        let flags = randomx::RandomXFlags::get_recommended_flags()
            & !randomx::RandomXFlags::JIT;
        let rx_cache = self.chain_state.get_cache([0u8; 32]);
        let vm = Arc::new(
            randomx::RandomXVM::new(flags, Some(rx_cache), None)
                .map_err(|e| dwow_core::Error::Custom(format!("RandomX VM: {}", e)))?,
        );
        let overlay_arc = Arc::new(std::sync::Mutex::new(overlay.clone()));
        let backend = Arc::new(dwow_chain::execution::TxBackend {
            overlay: Arc::clone(&overlay_arc),
            store: self.chain_state.store.clone(),
            height: BlockHeight::GENESIS,
            vm,
        });
        let mut runtime = dwow_core::runtime::vm_runtime::Runtime::new(
            wasm, backend, contract_id, BlockHeight::GENESIS,
            BlockTarget::MAX, dwow_sdk::tx::TransactionHash::none(), 0,
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "Runtime::new for deploy: {}", e,
        )))?;
        runtime.deploy(&[]).map_err(|e| dwow_core::Error::Custom(format!(
            "deploy __initialize: {}", e,
        )))?;
        drop(runtime);

        overlay = overlay_arc.lock().unwrap().clone();
        let batch = overlay.state.aggregate().unwrap_or_default();
        contracts_tree.apply_batch(batch)
            .map_err(|e| dwow_core::Error::Custom(format!("apply_batch: {}", e)))?;
        Ok(contract_id)
    }

    /// Deploy with explicit initialization payload.
    pub async fn deploy_with_ix(
        &self,
        harness: &dyn ContractHarness,
        name: &str,
        wasm: &[u8],
        ix: &[u8],
    ) -> Result<ContractId> {
        harness.verify_zk_coverage()?;
        let contract_id = derive_contract_id_from_name(name);
        // same as deploy() but passes `ix` to runtime.deploy(ix)
        let contracts_tree = self.chain_state.store.contracts_tree().clone();
        let mut overlay = sled_overlay::SledTreeOverlay::new(&contracts_tree);
        overlay.state.cache.insert(
            sled::IVec::from(contract_id.to_bytes().as_slice()),
            sled::IVec::from(wasm),
        );
        let flags = randomx::RandomXFlags::get_recommended_flags()
            & !randomx::RandomXFlags::JIT;
        let rx_cache = self.chain_state.get_cache([0u8; 32]);
        let vm = Arc::new(
            randomx::RandomXVM::new(flags, Some(rx_cache), None)
                .map_err(|e| dwow_core::Error::Custom(format!("RandomX VM: {}", e)))?,
        );
        let overlay_arc = Arc::new(std::sync::Mutex::new(overlay.clone()));
        let backend = Arc::new(dwow_chain::execution::TxBackend {
            overlay: Arc::clone(&overlay_arc),
            store: self.chain_state.store.clone(),
            height: BlockHeight::GENESIS,
            vm,
        });
        let mut runtime = dwow_core::runtime::vm_runtime::Runtime::new(
            wasm, backend, contract_id, BlockHeight::GENESIS,
            BlockTarget::MAX, dwow_sdk::tx::TransactionHash::none(), 0,
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "Runtime::new for deploy: {}", e,
        )))?;
        runtime.deploy(ix).map_err(|e| dwow_core::Error::Custom(format!(
            "deploy __initialize: {}", e,
        )))?;
        drop(runtime);
        overlay = overlay_arc.lock().unwrap().clone();
        let batch = overlay.state.aggregate().unwrap_or_default();
        contracts_tree.apply_batch(batch)
            .map_err(|e| dwow_core::Error::Custom(format!("apply_batch: {}", e)))?;
        Ok(contract_id)
    }

    /// Current block height.
    pub fn height(&self) -> BlockHeight {
        self.chain_state.get_height()
    }

    /// Start building a new block at the next height.
    pub fn block(&self) -> Result<HeavyweightBlock> {
        let h = self.height();
        let next = h.succ();
        let reward = dwow_sdk::blockchain::expected_reward(next);
        Ok(HeavyweightBlock {
            chain: self,
            height: next,
            reward,
            contract_txs: Vec::new(),
            uncles: Vec::new(),
        })
    }

    /// Build a coinbase at a specific height with a specific reward.
    /// Needed by uncle tests that need to construct coinbases with custom
    /// rewards (e.g., over-reward rejection tests).
    pub async fn build_coinbase_for_height(
        &self,
        height: BlockHeight,
        reward: BlockReward,
    ) -> Result<CoinbaseResult> {
        self.build_coinbase_inner(height, reward).await
    }

    /// Get the expected PoW target for the next block from chain consensus.
    pub fn expected_target(&self, next_height: BlockHeight) -> BlockTarget {
        self.chain_state.consensus.lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_next_work_required(&self.chain_state.store, next_height)
    }

    // ── Internal helpers ─────────────────────────────────────────────

    async fn build_coinbase_inner(
        &self,
        height: BlockHeight,
        reward: BlockReward,
    ) -> Result<CoinbaseResult> {
        let mgr = crate::accounts::AccountManager::open(
            &self.keys_path,
            dwow_sdk::crypto::keypair::Network::Testnet,
            "node0",
        ).map_err(|e| dwow_core::Error::Custom(format!("open test keys: {}", e)))?;
        let recipient = crate::accounts::MiningRecipient::from_account(&mgr, height)
            .map_err(|e| dwow_core::Error::Custom(format!("MiningRecipient: {}", e)))?;
        drop(mgr);

        let (coinbase, _pi, pow_reward_call, coin_blind) =
            crate::registry::model::build_linear_coinbase(
                recipient.clone(), reward, &self.linear_zk, height,
            ).await?;

        let tx = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call],
            lock_time: 0,
            nullifiers: vec![coinbase.nullifier],
            witness: vec![],
        };
        Ok(CoinbaseResult {
            tx,
            recipient,
            coin_value: reward.get(),
            coin_commitment: coinbase.coin,
            nullifier: coinbase.nullifier,
            coin_blind,
        })
    }

}

// ── HeavyweightBlock ───────────────────────────────────────────────────────

/// Fluent per-block builder. Accumulates contract calls, then seals and
/// submits through `accept_block`. Consumed by `submit()`.
///
/// Created by `HeavyweightPipeline::block()`.
pub struct HeavyweightBlock<'c> {
    chain: &'c HeavyweightPipeline,
    height: BlockHeight,
    reward: BlockReward,
    contract_txs: Vec<dwow_chain::Transaction>,
    uncles: Vec<dwow_chain::UncleBlock>,
}

impl<'c> HeavyweightBlock<'c> {
    /// Add one contract call to this block.
    ///
    /// Takes the contract_id and the raw outputs of a harness method.
    /// Internally: strict_zk check, build_witness,
    /// construct tx, accumulate.
    pub fn with_call(
        &mut self,
        contract_id: ContractId,
        harness: &dyn ContractHarness,
        call_data: &[u8],
        proofs: Vec<Proof>,
    ) -> Result<&mut Self> {
        let circuits = harness.circuits();
        // strict_zk guards: ZK contracts MUST have proofs, MUST NOT use Schnorr.
        // Schnorr signatures are prohibited per contract-standards.md §3.
        // The Transaction struct has no signatures field — this is a structural
        // guard. The tripwire_no_schnorr_signature_pubkeys test catches metadata leaks.
        if !circuits.is_empty() && proofs.is_empty() {
            if self.chain.strict_zk {
                return Err(dwow_core::Error::Custom(format!(
                    "with_call() on ZK contract '{}' ({} circuits: [{}]) with empty proofs",
                    harness.name(), circuits.len(), circuits.join(", ")
                )));
            }
            eprintln!(
                "[blockchain] WARNING: call on ZK contract '{}' with empty proofs",
                harness.name()
            );
        }

        // Raw call data: [fn_code] + params. The execution layer
        // (execution.rs) extracts the DarkLeaf call tree from the witness
        // and passes it to WASM — no client-side wrapping needed.
        let mut tx = super::harness::build_contract_tx(contract_id, call_data.to_vec());
        tx.witness = super::heavyweight_pipeline::build_witness(
            contract_id, call_data, proofs,
        );

        self.contract_txs.push(tx);
        Ok(self)
    }

    /// Add an uncle block to this block.
    pub fn with_uncle(&mut self, uncle: dwow_chain::UncleBlock) -> &mut Self {
        self.uncles.push(uncle);
        self
    }

    /// Add multiple uncle blocks.
    pub fn with_uncles(&mut self, uncles: Vec<dwow_chain::UncleBlock>) -> &mut Self {
        self.uncles.extend(uncles);
        self
    }

    /// Append a FeeCollectV1 transaction to close the merkle tree.
    ///
    /// Scans accumulated contract_txs for FeeV1 calls (NATIVE_TOKEN_CONTRACT_ID,
    /// selector 0x00). If any are found, builds a FeeCollectV1 transaction using
    /// the pipeline's cached ZK proving key and appends it to contract_txs.
    /// Returns `&mut Self` for chaining. If no FeeV1 calls exist, does nothing.
    ///
    /// Every production block includes FeeCollectV1 as the final transaction.
    /// Heavyweight v2 tests mirror this: every block in every test calls
    /// `with_fee_collect()` to exercise the full merkle tree open/close lifecycle.
    pub fn with_fee_collect(&mut self) -> Result<&mut Self> {
        let fee_txs: Vec<dwow_chain::Transaction> = self.contract_txs.iter()
            .filter(|tx| tx.contract_calls.iter().any(|c|
                c.contract_id == *NATIVE_TOKEN_CONTRACT_ID
                && c.data.first() == Some(&0x00)
            ))
            .cloned()
            .collect();

        if fee_txs.is_empty() {
            return Ok(self);
        }

        let mgr = crate::accounts::AccountManager::open(
            &self.chain.keys_path,
            dwow_sdk::crypto::keypair::Network::Testnet,
            "node0",
        ).map_err(|e| dwow_core::Error::Custom(format!("open test keys: {}", e)))?;
        let recipient = crate::accounts::MiningRecipient::from_account(&mgr, self.height)
            .map_err(|e| dwow_core::Error::Custom(format!("MiningRecipient: {}", e)))?;
        drop(mgr);

        let fee_collect_tx = crate::registry::model::build_fee_collect_tx(
            &recipient, &fee_txs, self.height, &self.chain.linear_zk,
        ).map_err(|e| dwow_core::Error::Custom(format!("build_fee_collect_tx: {}", e)))?;

        if let Some(tx) = fee_collect_tx {
            self.contract_txs.push(tx);
        }
        Ok(self)
    }

    /// Build just the coinbase for this block (without submitting).
    ///
    /// Needed when contract calls reference coinbase coin parameters
    /// (coin_commitment, nullifier, coin_blind). The caller builds the
    /// coinbase first, uses its parameters to construct call_data, then
    /// calls `submit_with_coinbase()`.
    pub async fn build_coinbase(&self) -> Result<CoinbaseResult> {
        self.chain.build_coinbase_inner(self.height, self.reward).await
    }

    /// Seal and submit the block through `accept_block`.
    ///
    /// Builds the coinbase, assembles all transactions, constructs the
    /// block, builds RandomX VM, and calls `accept_block`.
    pub async fn submit(&mut self) -> Result<BlockHeight> {
        eprintln!("[blockchain] building coinbase for height {}...", self.height);
        let t0 = std::time::Instant::now();
        let cb = self.chain.build_coinbase_inner(self.height, self.reward).await?;
        eprintln!("[blockchain] coinbase built ({:.1}s)", t0.elapsed().as_secs_f64());
        self.submit_inner(cb.tx).await
    }

    /// Submit with a pre-built coinbase (for tests that needed coinbase
    /// parameters before constructing contract calls).
    pub async fn submit_with_coinbase(&mut self, coinbase_tx: dwow_chain::Transaction) -> Result<BlockHeight> {
        self.submit_inner(coinbase_tx).await
    }

    async fn submit_inner(&mut self, coinbase_tx: dwow_chain::Transaction) -> Result<BlockHeight> {
        use super::harness::{build_test_block, build_test_block_with_uncles};
        use super::heavyweight_pipeline::{build_accept_vm, mine_test_nonce};

        let tx_count = self.contract_txs.len();
        let uncle_count = self.uncles.len();
        let mut all_txs = Vec::with_capacity(1 + tx_count);
        all_txs.push(coinbase_tx);
        all_txs.extend(std::mem::take(&mut self.contract_txs));

        eprintln!("[blockchain] assembling block height {} ({} txs, {} uncles)...",
            self.height, tx_count + 1, uncle_count);

        let target = self.chain.expected_target(self.height);
        let uncles = std::mem::take(&mut self.uncles);
        let mut block = if uncles.is_empty() {
            build_test_block(&self.chain.chain_state, self.height, all_txs)
        } else {
            build_test_block_with_uncles(
                &self.chain.chain_state, self.height, all_txs, &uncles,
            )
        };
        block.header.target = target;

        eprintln!("[blockchain] building RandomX VM...");
        let t0 = std::time::Instant::now();
        let vm = build_accept_vm(&block)?;
        eprintln!("[blockchain] RandomX VM ready ({:.1}s)", t0.elapsed().as_secs_f64());

        if target < BlockTarget::MAX {
            block.header.nonce = mine_test_nonce(&block, &vm, target);
        }

        let current_height = self.height.pred()
            .expect("block height must have predecessor");
        eprintln!("[blockchain] submitting to accept_block...");
        let t0 = std::time::Instant::now();
        let outcome = crate::block_acceptor::accept_block(
            &self.chain.chain_state,
            &block,
            &uncles,
            &vm,
            current_height,
            target,
            None,
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "accept_block at height {}: {}", self.height, e
        )))?;

        match outcome {
            dwow_chain::BlockConnectOutcome::CanonicalExtension { new_height } => {
                eprintln!("[blockchain] block accepted at height {} ({:.1}s, {} txs)",
                    new_height, t0.elapsed().as_secs_f64(), tx_count + 1);
                Ok(new_height)
            }
            _ => {
                eprintln!("[blockchain] block processed ({:.1}s, non-canonical)",
                    t0.elapsed().as_secs_f64());
                Ok(self.height)
            }
        }
    }
}

/// Derive a deterministic ContractId from a contract name.
/// Same algorithm as HeavyweightPipeline::derive_contract_id().
fn derive_contract_id_from_name(name: &str) -> ContractId {
    let mut hash: u64 = 0;
    for &b in name.as_bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(b as u64);
    }
    let mut bytes = [0u8; 32];
    bytes[0..8].copy_from_slice(&hash.to_le_bytes());
    ContractId::from_bytes(bytes).expect("valid u64 contract id")
}
