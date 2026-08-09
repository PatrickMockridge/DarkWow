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

use std::sync::{Arc, Mutex,
    atomic::{AtomicU32, Ordering},
};

use dwow_chain::{CChainState, FinalityConfig, PoWConfig};
use dwow_core::zk::Proof;
use dwow_core::Result;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, FeeAmount};
use dwow_sdk::crypto::{ContractId, NATIVE_TOKEN_CONTRACT_ID};
use dwow_sdk::pasta::pallas;
use dwow_sdk::pasta::group::{Group, GroupEncoding};
use dwow_contract_test_harness::harness::ContractHarness;

/// Genesis contract names that SHALL NOT be re-deployed via chain.deploy().
/// Spec §5.1-5.8, RG-7.
const GENESIS_CONTRACT_NAMES: &[&str] = &[
    "native_token", "deployooor", "identity", "attestation", "multisig",
    "oracle", "promissory_note", "purse", "box",
];

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
    /// Optional test log file writer. Set by the test runner to capture
    /// all block-assembly and integrity-check output to a file.
    pub log_file: Option<Mutex<std::fs::File>>,
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

        Ok(Self { db, chain_state, linear_zk, keys_path, log_file: None })
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
        crate::init_genesis(&self.chain_state, recipient, magic_bytes).await?;
        // Initialize the supply tracking tree with the genesis coinbase reward.
        // cumulative_supply() reads from supply_chain at key b"latest_supply".
        let initial_supply = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
        self.chain_state.store.supply_chain.insert(
            b"latest_supply".to_vec(),
            initial_supply.get().to_le_bytes().to_vec(),
        )?;

        // Verify genesis properly initialized the fee commitment accumulator.
        // Early-fail prevents confusing "accumulator not found" errors in
        // downstream FeeV2 tests that would mask the root cause.
        // Per genesis.md Structural Identity §Fee lifecycle.
        let cid = *NATIVE_TOKEN_CONTRACT_ID;
        let acc_data = self.query_contract_state(cid, "info", b"fee_commit_acc")?
            .ok_or_else(|| dwow_core::Error::Custom(
                "GENESIS FAIL: fee_commit_accumulator key not found — \
                 init_contract may not have run".into()
            ))?;
        if acc_data.len() != 32 {
            return Err(dwow_core::Error::Custom(format!(
                "GENESIS FAIL: fee_commit_accumulator wrong size: {} bytes (expected 32)",
                acc_data.len()
            )));
        }
        let acc_point: pallas::Point = Option::from(
            pallas::Point::from_bytes(&acc_data[..32].try_into().map_err(|_|
                dwow_core::Error::Custom("fee_commit_accumulator: invalid point encoding".into())
            )?)
        ).ok_or_else(|| dwow_core::Error::Custom(
            "GENESIS FAIL: invalid fee_commit_accumulator point encoding".into()
        ))?;
        if acc_point != pallas::Point::identity() {
            return Err(dwow_core::Error::Custom(format!(
                "GENESIS FAIL: fee_commit_accumulator is not Identity (got: {:?}) — \
                 genesis initialization may be corrupt", acc_point
            )));
        }

        Ok(())
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
        // RG-7: Genesis contracts SHALL NOT be re-deployed
        if GENESIS_CONTRACT_NAMES.contains(&name) {
            return Err(dwow_core::Error::Custom(format!(
                "Cannot deploy genesis contract '{}' — use its static ContractId",
                name
            )));
        }
        if let Err(e) = harness.verify_zk_coverage() {
            self.log(&format!("WARN [integrity_checks]: PI-4 ZK coverage check failed for '{}' — {}", name, e));
        }
        let contract_id = derive_contract_id_from_name(name);
        let contracts_tree = self.chain_state.store.contracts_tree().clone();
        let mut overlay = sled_overlay::SledTreeOverlay::new(&contracts_tree);
        overlay.state.cache.insert(
            sled::IVec::from(contract_id.to_bytes().as_slice()),
            sled::IVec::from(wasm),
        );

        let flags = randomx::RandomXFlags::get_recommended_flags()
            & !randomx::RandomXFlags::JIT;
        let rx_cache = self.chain_state.get_cache([0u8; 32])
            .map_err(|e| dwow_core::Error::Custom(format!("RandomX cache: {}", e)))?;
        let vm = Arc::new(
            randomx::RandomXVM::new(flags, Some(rx_cache), None)
                .map_err(|e| dwow_core::Error::Custom(format!("RandomX VM: {}", e)))?,
        );
        let overlay_arc = Arc::new(std::sync::Mutex::new(overlay.clone()));
        let mut deploy_anchor = dwow_sdk::crypto::MerkleTree::new(1);
        deploy_anchor.append(dwow_sdk::crypto::MerkleNode::from_base(pallas::Base::from(2u64)));
        let backend = Arc::new(dwow_chain::execution::TxBackend {
            overlay: Arc::clone(&overlay_arc),
            store: self.chain_state.store.clone(),
            height: BlockHeight::GENESIS,
            vm,
            block_anchor_tree: Arc::new(Mutex::new(deploy_anchor)),
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
        // RG-7: Genesis contracts SHALL NOT be re-deployed
        if GENESIS_CONTRACT_NAMES.contains(&name) {
            return Err(dwow_core::Error::Custom(format!(
                "Cannot deploy genesis contract '{}' — use its static ContractId",
                name
            )));
        }
        if let Err(e) = harness.verify_zk_coverage() {
            self.log(&format!("WARN [integrity_checks]: PI-4 ZK coverage check failed for '{}' — {}", name, e));
        }
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
        let rx_cache = self.chain_state.get_cache([0u8; 32])
            .map_err(|e| dwow_core::Error::Custom(format!("RandomX cache: {}", e)))?;
        let vm = Arc::new(
            randomx::RandomXVM::new(flags, Some(rx_cache), None)
                .map_err(|e| dwow_core::Error::Custom(format!("RandomX VM: {}", e)))?,
        );
        let overlay_arc = Arc::new(std::sync::Mutex::new(overlay.clone()));
        let mut deploy_anchor = dwow_sdk::crypto::MerkleTree::new(1);
        deploy_anchor.append(dwow_sdk::crypto::MerkleNode::from_base(pallas::Base::from(2u64)));
        let backend = Arc::new(dwow_chain::execution::TxBackend {
            overlay: Arc::clone(&overlay_arc),
            store: self.chain_state.store.clone(),
            height: BlockHeight::GENESIS,
            vm,
            block_anchor_tree: Arc::new(Mutex::new(deploy_anchor)),
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
    pub fn block(&self) -> Result<HeavyweightBlock<'_>> {
        let h = self.height();
        let next = h.succ();
        let reward = dwow_sdk::blockchain::expected_reward(next);
        Ok(HeavyweightBlock {
            chain: self,
            height: next,
            reward,
            contract_txs: Vec::new(),
            uncles: Vec::new(),
            block_hash: None,
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
            .unwrap_or(BlockTarget::MAX)
    }

    // ── State inspection API (RG-8, spec §7.2 PR-3) ─────────────────

    /// Query a value from the contracts sled tree.
    pub fn query_contracts_tree(
        &self,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let tree = self.chain_state.store.contracts_tree();
        tree.get(key)
            .map_err(|e| dwow_core::Error::Custom(format!(
                "query_contracts_tree: sled get: {}", e
            )))
            .map(|opt| opt.map(|iv| iv.to_vec()))
    }

    /// Query contract state from a named sled tree.
    pub fn query_sled_tree(
        &self,
        tree_name: &str,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let tree = self.db.open_tree(tree_name)
            .map_err(|e| dwow_core::Error::Custom(format!(
                "query_sled_tree: open tree '{}': {}", tree_name, e
            )))?;
        tree.get(key)
            .map_err(|e| dwow_core::Error::Custom(format!(
                "query_sled_tree: sled get: {}", e
            )))
            .map(|opt| opt.map(|iv| iv.to_vec()))
    }

    /// Query a contract's internal state using the same handle derivation
    /// as the WASM runtime (cid.hash_state_id(tree_name)).
    /// Used for cross-block state verification (HAZOP finding — compound correctness).
    /// Spec: §6 ST-2.
    pub fn query_contract_state(
        &self,
        cid: ContractId,
        tree_name: &str,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        // WASM execution stores state in the contracts sled tree with
        // composite keys: hash_state_id(cid, tree_name) || key.
        // query_contract_state mirrors this: it builds the same composite
        // key and reads from the contracts tree, NOT from a separate
        // per-contract sled tree (which would always be empty).
        let handle = cid.hash_state_id(tree_name);
        let mut composite_key = handle.to_vec();
        composite_key.extend_from_slice(key);
        self.query_contracts_tree(&composite_key)
    }

    /// Current cumulative supply computed from all block rewards.
    /// Sums expected_reward(h) for h in 1..=current_height.
    pub fn cumulative_supply(&self) -> u64 {
        let h = self.height();
        (1..=h.get()).map(|h| {
            dwow_sdk::blockchain::expected_reward(
                dwow_sdk::blockchain::BlockHeight::new(h)
            ).get()
        }).sum()
    }

    /// Write a message to stderr and to the test log file (if configured).
    /// Flushes after every write so partial output survives a panic.
    pub fn log(&self, msg: &str) {
        eprintln!("{}", msg);
        if let Some(ref mtx) = self.log_file {
            use std::io::Write;
            if let Ok(mut f) = mtx.lock() {
                let _ = writeln!(f, "{}", msg);
                let _ = f.flush();
            }
        }
    }

    /// Return the per-block derived mining keypair for coinbase at `height`.
    /// Uses the same `MiningRecipient::from_account()` derivation as
    /// build_coinbase_inner() — needed when FeeV1 spends a coinbase coin
    /// so the ZK proof produces a coin commitment matching the coinbase key.
    pub fn mining_keypair(&self, height: BlockHeight) -> dwow_sdk::crypto::Keypair {
        use dwow_sdk::crypto::{Keypair, PublicKey, SecretKey};
        use dwow_sdk::crypto::keypair::Network;
        let mgr = crate::accounts::AccountManager::open(
            &self.keys_path, Network::Testnet, "node0",
        ).expect("open test keys for mining keypair");
        let recipient = crate::accounts::MiningRecipient::from_account(&mgr, height)
            .expect("MiningRecipient");
        let secret: SecretKey = recipient.secret().clone().into();
        Keypair { secret, public: recipient.public() }
    }

    /// Verify the block hash chain is continuous from height 2 to current.
    pub fn block_hash_chain_continuous(&self) -> Result<bool> {
        let current = self.height();
        if current <= BlockHeight::new(1) {
            return Ok(true); // only genesis exists
        }
        for h in 2..=current.get() {
            let height = BlockHeight::new(h);
            let block = self.chain_state.store.get_block(height)
                .map_err(|e| dwow_core::Error::Custom(format!(
                    "block_hash_chain: get block at height {}: {}", h, e
                )))?;
            let prev_block = self.chain_state.store.get_block(
                BlockHeight::new(h - 1)
            ).map_err(|e| dwow_core::Error::Custom(format!(
                "block_hash_chain: get prev block at height {}: {}", h - 1, e
            )))?;
            if block.header.previous != self.chain_state.hash_block_with_cached_vm(&prev_block).expect("hash failed") {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Get the block hash at a given height.
    pub fn block_hash_at(&self, height: BlockHeight) -> Result<Option<blake3::Hash>> {
        let block = self.chain_state.store.get_block(height)
            .map_err(|e| dwow_core::Error::Custom(format!(
                "block_hash_at height {}: {}", height, e
            )))?;
        Ok(Some(self.chain_state.hash_block_with_cached_vm(&block).expect("hash failed")))
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
    /// Block hash stored after successful submission (RG-8, spec §7.2 PR-5)
    block_hash: Option<blake3::Hash>,
}

impl<'c> HeavyweightBlock<'c> {
    /// Add one contract call to this block.
    ///
    /// Takes the contract_id and the raw outputs of a harness method.
    /// Internally: build_witness, construct tx, accumulate.
    /// ZK gating is enforced by the uniform runner's submit_block(), not here.
    /// spec §7.2 PR-6: with_call() is a data accumulation method, not a security gate.
    pub fn with_call(
        &mut self,
        contract_id: ContractId,
        harness: &dyn ContractHarness,
        call_data: &[u8],
        proofs: Vec<Proof>,
    ) -> Result<&mut Self> {
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
    /// Appends FeeCollectV1 when FeeV2 (0x08) calls exist in the block.
    /// When no FeeV2 calls exist, FeeCollectV1 is omitted — matching the
    /// production miner's conditional append. Zero-fee FeeCollectV1 is
    /// rejected as a zero-value replay attack (fee-spec.md §4.4).
    pub fn with_fee_collect(&mut self) -> Result<&mut Self> {
        let fee_txs: Vec<dwow_chain::Transaction> = self.contract_txs.iter()
            .filter(|tx| tx.contract_calls.iter().any(|c|
                c.contract_id == *NATIVE_TOKEN_CONTRACT_ID
                && c.data.first() == Some(&0x08)  // FeeV2
            ))
            .cloned()
            .collect();

        let mgr = crate::accounts::AccountManager::open(
            &self.chain.keys_path,
            dwow_sdk::crypto::keypair::Network::Testnet,
            "node0",
        ).map_err(|e| dwow_core::Error::Custom(format!("open test keys: {}", e)))?;
        let recipient = crate::accounts::MiningRecipient::from_account(&mgr, self.height)
            .map_err(|e| dwow_core::Error::Custom(format!("MiningRecipient: {}", e)))?;
        drop(mgr);

        let fee_call_count = fee_txs.len() as u64;
        let total_fees = FeeAmount::new(fee_call_count * 42_000_000); // DEFAULT_FEE per FeeV2 call
        let fee_collect_tx = crate::registry::model::build_fee_collect_tx(
            &recipient, &fee_txs, self.height, &self.chain.linear_zk, total_fees,
        ).map_err(|e| dwow_core::Error::Custom(format!("build_fee_collect_tx: {}", e)))?;

        // Append FeeCollectV1 only when FeeV1 calls exist in the block.
        // When no FeeV1 calls exist, omit FeeCollectV1 — the production miner
        // does this (lib.rs:1358) and the validator allows it (validation.rs:386).
        // Zero-fee FeeCollectV1 is rejected as a zero-value replay attack (§3.13).
        if let Some(tx) = fee_collect_tx {
            self.contract_txs.push(tx);
        }
        Ok(self)
    }

    /// Get the block hash after successful submission (RG-8, spec §7.2 PR-5).
    pub fn block_hash(&self) -> Option<blake3::Hash> {
        self.block_hash
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
        self.chain.log(&format!("[blockchain] building coinbase for height {}...", self.height));
        let t0 = std::time::Instant::now();
        let cb = self.chain.build_coinbase_inner(self.height, self.reward).await?;
        self.chain.log(&format!("[blockchain] coinbase built ({:.1}s)", t0.elapsed().as_secs_f64()));
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

        self.chain.log(&format!("[blockchain] assembling block height {} ({} txs, {} uncles)...",
            self.height, tx_count + 1, uncle_count));

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

        self.chain.log("[blockchain] building RandomX VM...");
        let t0 = std::time::Instant::now();
        let vm = build_accept_vm(&block)?;
        self.chain.log(&format!("[blockchain] RandomX VM ready ({:.1}s)", t0.elapsed().as_secs_f64()));

        if target < BlockTarget::MAX {
            block.header.nonce = mine_test_nonce(&block, &vm, target)?;
        }

        let current_height = self.height.pred()
            .expect("block height must have predecessor");
        self.chain.log("[blockchain] submitting to accept_block...");
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

        let block_hash = block.hash_with_vm(&vm).expect("hash failed");
        match outcome {
            dwow_chain::BlockConnectOutcome::CanonicalExtension { new_height } => {
                self.chain.log(&format!("[blockchain] block accepted at height {} ({:.1}s, {} txs)",
                    new_height, t0.elapsed().as_secs_f64(), tx_count + 1));
                self.block_hash = Some(block_hash);
                Ok(new_height)
            }
            _ => {
                self.chain.log(&format!("[blockchain] block processed ({:.1}s, non-canonical)",
                    t0.elapsed().as_secs_f64()));
                self.block_hash = Some(block_hash);
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
