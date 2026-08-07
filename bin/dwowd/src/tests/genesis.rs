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

//! GenesisHarness — Reusable baseline chain for linear blockchain testing.
//!
//! Creates a temp sled DB and CChainState with all 9 genesis contracts
//! pre-deployed. Uses `store.set_contract_data()` to store WASM bytes
//! directly — no block mining needed for test setup.
//!
//! Updated for CChainState (commit 597691582 refactor — replaces LinearBlockchain).

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use dwow_chain::{CChainState, FinalityConfig, PoWConfig};
use dwow_core::Result;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, BlockTimestamp, BlockVersion, MoneroBlockHeight, SupplyAmount};
use dwow_sdk::pasta::pallas;
use dwow_sdk::pasta::group::GroupEncoding;
use dwow_sdk::pasta::group::Group;
use dwow_sdk::crypto::{
    ATTESTATION_CONTRACT_ID, BOX_CONTRACT_ID, ContractId, DEPLOYOOOR_CONTRACT_ID,
    IDENTITY_CONTRACT_ID, MULTISIG_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID,
    ORACLE_CONTRACT_ID, PROMISSORY_NOTE_CONTRACT_ID, PURSE_CONTRACT_ID,
};

/// Reusable baseline chain with all 9 genesis contracts pre-deployed.
pub struct GenesisHarness {
    /// Temp sled database
    pub db: Arc<sled::Db>,
    /// Single authoritative chain state (replaces LinearBlockchain)
    pub chain_state: Arc<CChainState>,
}

impl GenesisHarness {
    /// Create the temp sled DB + CChainState shared by both constructors.
    fn new_bare() -> Result<Self> {
        static GEN_COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = GEN_COUNTER.fetch_add(1, Ordering::Relaxed);
        let db_dir = std::env::temp_dir()
            .join(format!("dwow_gen_test_{}_{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&db_dir);
        let db = sled::Config::new()
            .path(&db_dir)
            .open()
            .map_err(|e| dwow_core::Error::Custom(format!("Failed to create sled DB: {}", e)))?;
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
        // CChainState::new() already returns Arc<CChainState>.

        Ok(Self { db, chain_state })
    }

    /// Harness with a completely EMPTY contracts tree — the production
    /// starting state of every node. Contracts materialize only by executing
    /// the genesis block (apply_genesis_deployments consensus rule), whether
    /// created locally via `init_genesis` or accepted from sync via
    /// `accept_block`. Use this for genesis-path tests.
    pub fn new_without_contracts() -> Result<Self> {
        Self::new_bare()
    }

    /// Create a new GenesisHarness with temp sled DB and CChainState.
    /// Deploys NativeToken and Deployooor WASM so the chain is ready
    /// for contract deployment.
    pub fn new() -> Result<Self> {
        let harness = Self::new_bare()?;
        let chain_state = &harness.chain_state;
        // Store WASM bytes directly — no deploy_contract() on CChainState.
        // Pattern from bin/dwowd/src/lib.rs:370-395.
        let deployooor_wasm = include_bytes!(
            "../../../../src/contract/deployooor/dwow_deployooor_contract.wasm"
        );
        chain_state.store.set_contract_data(
            &DEPLOYOOOR_CONTRACT_ID.to_bytes(),
            deployooor_wasm,
        ).map_err(|e| dwow_core::Error::Custom(format!("Failed to store deployooor WASM: {}", e)))?;

        let native_token_wasm = include_bytes!(
            "../../../../src/contract/native_token/dwow_native_token_contract.wasm"
        );
        chain_state.store.set_contract_data(
            &NATIVE_TOKEN_CONTRACT_ID.to_bytes(),
            native_token_wasm,
        ).map_err(|e| dwow_core::Error::Custom(format!("Failed to store native_token WASM: {}", e)))?;

        let identity_wasm = include_bytes!(
            "../../../../src/contract/identity/dwow_identity_contract.wasm"
        );
        chain_state.store.set_contract_data(
            &IDENTITY_CONTRACT_ID.to_bytes(),
            identity_wasm,
        ).map_err(|e| dwow_core::Error::Custom(format!("Failed to store identity WASM: {}", e)))?;

        let oracle_wasm = include_bytes!(
            "../../../../src/contract/oracle/dwow_oracle_contract.wasm"
        );
        chain_state.store.set_contract_data(
            &ORACLE_CONTRACT_ID.to_bytes(),
            oracle_wasm,
        ).map_err(|e| dwow_core::Error::Custom(format!("Failed to store oracle WASM: {}", e)))?;

        let attestation_wasm = include_bytes!(
            "../../../../src/contract/attestation/dwow_attestation_contract.wasm"
        );
        chain_state.store.set_contract_data(
            &ATTESTATION_CONTRACT_ID.to_bytes(),
            attestation_wasm,
        ).map_err(|e| dwow_core::Error::Custom(format!("Failed to store attestation WASM: {}", e)))?;

        // --- Ecosystem genesis contracts (counters 3, 8, 9, 10) ---

        let promissory_note_wasm = include_bytes!(
            "../../../../src/contract/promissory_note/dwow_promissory_note_contract.wasm"
        );
        chain_state.store.set_contract_data(
            &PROMISSORY_NOTE_CONTRACT_ID.to_bytes(),
            promissory_note_wasm,
        ).map_err(|e| dwow_core::Error::Custom(format!("Failed to store PN WASM: {}", e)))?;

        let purse_wasm = include_bytes!(
            "../../../../src/contract/purse/dwow_purse_contract.wasm"
        );
        chain_state.store.set_contract_data(
            &PURSE_CONTRACT_ID.to_bytes(),
            purse_wasm,
        ).map_err(|e| dwow_core::Error::Custom(format!("Failed to store purse WASM: {}", e)))?;

        let box_wasm = include_bytes!(
            "../../../../src/contract/box/dwow_box_contract.wasm"
        );
        chain_state.store.set_contract_data(
            &BOX_CONTRACT_ID.to_bytes(),
            box_wasm,
        ).map_err(|e| dwow_core::Error::Custom(format!("Failed to store box WASM: {}", e)))?;

        let multisig_wasm = include_bytes!(
            "../../../../src/contract/multisig/dwow_multisig_contract.wasm"
        );
        chain_state.store.set_contract_data(
            &MULTISIG_CONTRACT_ID.to_bytes(),
            multisig_wasm,
        ).map_err(|e| dwow_core::Error::Custom(format!("Failed to store multisig WASM: {}", e)))?;

        Ok(harness)
    }

    /// Store a WASM contract in the chain state so it can be looked up.
    pub fn deploy_contract(&self, wasm: &[u8], contract_id: ContractId) -> Result<()> {
        self.chain_state.store.set_contract_data(&contract_id.to_bytes(), wasm)
            .map_err(|e| dwow_core::Error::Custom(format!("Failed to store contract WASM: {}", e)))
    }

    /// Get current block height.
    pub fn block_height(&self) -> BlockHeight {
        self.chain_state.get_height()
    }

    /// Query a contract's internal state using the same handle derivation
    /// as the WASM runtime (cid.hash_state_id(tree_name)).
    /// Mirrors HeavyweightPipeline::query_contract_state().
    pub fn query_contract_state(
        &self,
        cid: ContractId,
        tree_name: &str,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let handle = cid.hash_state_id(tree_name);
        let handle_str = format!("{:?}", handle);
        let tree = self.db.open_tree(&handle_str)
            .map_err(|e| dwow_core::Error::Custom(format!(
                "query_contract_state: open tree '{}': {}", handle_str, e
            )))?;
        tree.get(key)
            .map_err(|e| dwow_core::Error::Custom(format!(
                "query_contract_state: sled get: {}", e
            )))
            .map(|opt| opt.map(|iv| iv.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Genesis determinism: two nodes with identical setup MUST produce
    /// identical genesis blocks. Per genesis.md §Genesis Block — timestamp=0,
    /// deterministic key derivation, deterministic blinds, deterministic ZK proof
    /// (via DWOW_DETERMINISTIC_ZK env var).
    ///
    /// Set DWOW_DETERMINISTIC_ZK=1 before running to eliminate the last
    /// OsRng source in ZK proof generation.
    #[test]
    fn test_genesis_determinism() {
        // Thread-safe flag — replaces OsRng with StdRng::seed_from_u64(0)
        // in ZK proof generation. Per MOC guardrail G2.
        dwow_native_token_contract::enable_deterministic_zk();

        smol::block_on(async {
            // Production starting state: EMPTY contracts tree. The genesis
            // block carries the 9 deployments; init_genesis materializes
            // them via the apply_genesis_deployments consensus rule.
            let har1 = GenesisHarness::new_without_contracts().expect("GenesisHarness 1");
            let har2 = GenesisHarness::new_without_contracts().expect("GenesisHarness 2");

            // Build identical MiningRecipient from the same test key.
            // Unique temp file per process to avoid parallel test collisions (Gap 3).
            let keys_toml = "[node0]\nwallet_secret = \
                \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
            let path = std::env::temp_dir()
                .join(format!("dwow_gen_det_{}.toml", std::process::id()));
            std::fs::write(&path, keys_toml).expect("write test keys");

            let mgr = crate::accounts::AccountManager::open(
                &path, dwow_sdk::crypto::keypair::Network::Testnet, "node0",
            ).expect("open test AccountManager");
            let recipient1 = crate::accounts::MiningRecipient::from_account(&mgr, BlockHeight::new(1))
                .expect("MiningRecipient 1");
            let recipient2 = crate::accounts::MiningRecipient::from_account(&mgr, BlockHeight::new(1))
                .expect("MiningRecipient 2");
            drop(mgr);
            let _ = std::fs::remove_file(&path);

            let magic_bytes = [0xDA, 0x57, 0x01, 0x57];

            // Create genesis on both harnesses
            let hash1 = crate::init_genesis(&har1.chain_state, recipient1, magic_bytes)
                .await.expect("init_genesis har1");
            let hash2 = crate::init_genesis(&har2.chain_state, recipient2, magic_bytes)
                .await.expect("init_genesis har2");

            assert_eq!(hash1, hash2,
                "GEN DET FAIL: genesis hash must be deterministic. \
                 hash1={} hash2={}", hash1, hash2);

            // MOC acceptance criteria AC4-AC9
            assert_eq!(har1.block_height(), BlockHeight::new(1));
            assert_eq!(har2.block_height(), BlockHeight::new(1));

            let block1 = har1.chain_state.get_block(BlockHeight::new(1)).expect("har1 block 1");
            let block2 = har2.chain_state.get_block(BlockHeight::new(1)).expect("har2 block 1");

            // AC5: total_reward == expected_reward(1) = INITIAL_REWARD
            let expected = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
            assert_eq!(block1.header.total_reward, expected, "AC5: total_reward");
            assert_eq!(block2.header.total_reward, expected, "AC5: total_reward");

            // AC6: previous == [0u8; 32]
            assert_eq!(block1.header.previous.as_bytes(), &[0u8; 32], "AC6: previous");
            assert_eq!(block2.header.previous.as_bytes(), &[0u8; 32], "AC6: previous");

            // AC7: timestamp == 0
            assert_eq!(block1.header.timestamp, BlockTimestamp::new(0), "AC7: timestamp");
            assert_eq!(block2.header.timestamp, BlockTimestamp::new(0), "AC7: timestamp");

            // AC8: nonce == 0
            assert_eq!(block1.header.nonce, 0, "AC8: nonce");
            assert_eq!(block2.header.nonce, 0, "AC8: nonce");

            // AC9: target == u32::MAX
            assert_eq!(block1.header.target, BlockTarget::MAX, "AC9: target");
            assert_eq!(block2.header.target, BlockTarget::MAX, "AC9: target");

            // AC-FEE-1/2: fee_commit_accumulator == Identity after genesis.
            // The zero-fee constructor: genesis has no prior fees, so the
            // Pedersen accumulator MUST start at the identity point.
            // Per fee-spec.md §5.6.2 and genesis.md Structural Identity §Fee lifecycle.
            let acc_data = har1.query_contract_state(
                *NATIVE_TOKEN_CONTRACT_ID, "info", b"fee_commit_acc",
            ).expect("AC-FEE-1: sled query must succeed")
             .expect("AC-FEE-1: fee_commit_accumulator key must exist after genesis");
            assert_eq!(acc_data.len(), 32,
                "AC-FEE-1: fee_commit_accumulator must be 32 bytes (pallas::Point)");
            let acc_point: pallas::Point = Option::from(
                pallas::Point::from_bytes(&acc_data[..32].try_into().unwrap())
            ).expect("AC-FEE-1: valid fee_commit_accumulator point");
            assert_eq!(acc_point, pallas::Point::identity(),
                "AC-FEE-2: fee_commit_accumulator must be Identity after genesis \
                 (no prior fees exist). Found non-Identity — init_contract or \
                 apply_pow_reward may not have seeded the accumulator.");

            // Verify accumulator identical across both harnesses (determinism)
            let acc_data2 = har2.query_contract_state(
                *NATIVE_TOKEN_CONTRACT_ID, "info", b"fee_commit_acc",
            ).expect("sled query").expect("key exists");
            assert_eq!(acc_data, acc_data2,
                "AC-FEE-DET: accumulator must be deterministic across genesis instances");

            // AC2: cumulative supply at height 1 (MoC gap fill)
            let sc1 = har1.chain_state.supply_chain.get(BlockHeight::new(1))
                .expect("supply_chain at height 1");
            assert_eq!(sc1.total_supply, SupplyAmount::new(expected.get()),
                "AC2: cumulative supply at genesis");

            // AC10: genesis carries the deployments — exactly 10 txs
            // (coinbase + 9 contract deployments, positions 1..=9).
            assert_eq!(block1.transactions.len(), 10,
                "AC10: genesis block = 1 coinbase + 9 deployment txs");
            assert_eq!(block2.transactions.len(), 10,
                "AC10: genesis block = 1 coinbase + 9 deployment txs");
            for tx in &block1.transactions[1..] {
                assert!(dwow_chain::execution::is_genesis_deployment_tx(tx),
                    "AC10: txs 1..=9 are genesis deployment txs");
            }

            // AC-FEE-3: Genesis block SHALL NOT contain FeeCollectV1 (0x06).
            // Per fee-spec §4.4: FeeCollectV1 SHALL be absent when total_fees == 0.
            // Genesis has zero prior transactions, so total_fees == 0.
            // Per genesis.md Structural Identity §Transaction ordering.
            for (i, tx) in block1.transactions.iter().enumerate() {
                for call in &tx.contract_calls {
                    assert_ne!(call.data.first(), Some(&0x06),
                        "AC-FEE-3: Genesis tx[{}] SHALL NOT contain FeeCollectV1 (0x06) \
                         — total_fees == 0 at genesis, per fee-spec §4.4", i);
                }
            }

            // AC11: all 9 contracts materialized byte-equal to the payloads
            // the genesis block carries; 7 manifests present.
            for (i, (cid, name)) in
                dwow_chain::execution::genesis_contracts().iter().enumerate()
            {
                let params: dwow_sdk::deploy::DeployParamsV1 = dwow_serial::deserialize(
                    &block1.transactions[i + 1].contract_calls[0].data[1..],
                ).expect("DeployParamsV1 decode");
                for har in [&har1, &har2] {
                    let stored = har.chain_state.store
                        .get_contract_data(&cid.to_bytes())
                        .expect("get_contract_data");
                    assert_eq!(stored, params.wasm_bincode,
                        "AC11: {name} WASM byte-equal to genesis payload");
                    let mut mkey = Vec::from(cid.to_bytes().as_slice());
                    mkey.extend_from_slice(b"_manifest");
                    let manifest = har.chain_state.store
                        .get_contract_data(&mkey)
                        .expect("get manifest");
                    assert_eq!(manifest, params.ix,
                        "AC11: {name} manifest matches genesis payload");
                }
            }
        });
    }

    /// Block creation: genesis → build height-2 block with PoWRewardV1 coinbase
    /// → submit through `accept_block` (production path, WASM executes).
    ///
    /// Pre-devnet ceiling: tests ONE block on top of genesis through the full
    /// accept_block pipeline. Multi-block chain growth, competing blocks, and
    /// uncle resolution belong in the docker pipeline (`test_pipeline.sh`).
    ///
    /// Assertions (MoC acceptance criteria):
    ///   AC2 — Cumulative supply at height 1 (bridge was exercised by genesis)
    ///   AC3 — Height-2 block through accept_block
    ///   AC4 — Hash chain continuity (block2.previous == hash(genesis))
    ///   AC5 — Cumulative supply bridge (S_2 == S_1 + C_2)
    ///   — Reward correctness, block retrievability, coinbase tx presence
    #[test]
    fn test_block_creation() {
        dwow_native_token_contract::enable_deterministic_zk();

        smol::block_on(async {
            // ---- Setup ----
            // Empty contracts tree — genesis carries and materializes the
            // 9 contracts (production path).
            let har = GenesisHarness::new_without_contracts().expect("GenesisHarness");

            let keys_toml = "[node0]\nwallet_secret = \
                \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
            let path = std::env::temp_dir()
                .join(format!("dwow_block_cr_{}.toml", std::process::id()));
            std::fs::write(&path, keys_toml).expect("write test keys");

            let mgr = crate::accounts::AccountManager::open(
                &path,
                dwow_sdk::crypto::keypair::Network::Testnet,
                "node0",
            )
            .expect("open test AccountManager");
            let recipient =
                crate::accounts::MiningRecipient::from_account(&mgr, BlockHeight::new(1))
                    .expect("MiningRecipient");
            let magic_bytes = [0xDA, 0x57, 0x01, 0x57];

            // ---- Height 1: Genesis ----
            crate::init_genesis(&har.chain_state, recipient.clone(), magic_bytes)
                .await
                .expect("init_genesis");
            assert_eq!(har.block_height(), BlockHeight::new(1));

            // AC-FEE-1/2: fee_commit_accumulator == Identity after genesis
            let acc_data = har.query_contract_state(
                *NATIVE_TOKEN_CONTRACT_ID, "info", b"fee_commit_acc",
            ).expect("AC-FEE-1: sled query must succeed")
             .expect("AC-FEE-1: fee_commit_accumulator key must exist after genesis");
            assert_eq!(acc_data.len(), 32,
                "AC-FEE-1: fee_commit_accumulator must be 32 bytes (pallas::Point)");
            let acc_point: pallas::Point = Option::from(
                pallas::Point::from_bytes(&acc_data[..32].try_into().unwrap())
            ).expect("AC-FEE-1: valid fee_commit_accumulator point");
            assert_eq!(acc_point, pallas::Point::identity(),
                "AC-FEE-2: fee_commit_accumulator must be Identity after genesis");

            // AC2: cumulative supply at height 1
            let sc1 = har.chain_state.supply_chain.get(BlockHeight::new(1))
                .expect("supply_chain at height 1");
            assert_eq!(
                sc1.total_supply,
                SupplyAmount::new(dwow_sdk::blockchain::expected_reward(BlockHeight::new(1)).get()),
                "AC2: S_1 == INITIAL_REWARD"
            );

            let gen_block = har.chain_state.get_block(BlockHeight::new(1)).expect("block 1");
            let gen_hash = har.chain_state.hash_block_with_cached_vm(&gen_block).expect("hash failed");

            // ---- Height 2: coinbase-only block via accept_block ----
            let height = BlockHeight::new(2);
            let reward = dwow_sdk::blockchain::expected_reward(height);

            let linear_zk =
                crate::registry::model::LinearPowRewardZk::new(har.chain_state.clone())
                    .await
                    .expect("LinearPowRewardZk");
            let (_coinbase, _public_inputs, pow_reward_call, _coin_blind) =
                crate::registry::model::build_linear_coinbase(
                    recipient,
                    reward,
                    &linear_zk,
                    height,
                )
                .await
                .expect("coinbase for height 2");

            let tx = dwow_chain::Transaction {
                version: BlockVersion::CURRENT,
                inputs: vec![],
                outputs: vec![],
                contract_calls: vec![pow_reward_call],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            };
            let merkle_root = tx.hash();

            let header = dwow_chain::BlockHeader {
                version: BlockVersion::CURRENT,
                previous: gen_hash,
                merkle_root,
                timestamp: BlockTimestamp::new(120), // TARGET_BLOCK_TIME after genesis
                target: BlockTarget::MAX,
                nonce: 0,
                height,
                uncle_merkle_root: [0u8; 32],
                total_reward: reward,
                randomx_key: dwow_chain::Miner::derive_key_from_height(height),
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: dwow_chain::PowSource::Native,
            };

            let block = dwow_chain::Block { header, transactions: vec![tx] };

            // RandomX VM — same construction as init_genesis
            let rx_flags = randomx::RandomXFlags::get_recommended_flags()
                & !randomx::RandomXFlags::JIT;
            let rx_cache = randomx::RandomXCache::new(
                rx_flags,
                &block.header.randomx_key,
            )
            .expect("RandomX cache");
            let vm = std::sync::Arc::new(
                randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
                    .expect("RandomX VM"),
            );

            // Submit through accept_block — production path, WASM executes
            crate::block_acceptor::accept_block(
                &har.chain_state,
                &block,
                &[],          // no uncles
                &vm,
                BlockHeight::new(1), // current_height = block.height - 1
                BlockTarget::MAX,     // target
                None,
            )
            .expect("AC3: accept_block height 2");

            // ---- Assertions ----
            assert_eq!(har.block_height(), BlockHeight::new(2), "height advanced to 2");

            // AC-FEE-4: Stranded-fee canary — accumulator must be Identity
            // after a zero-fee height-2 block. If the accumulator were
            // non-Identity here, apply_pow_reward in block 2 silently
            // discarded stranded fees from genesis. This assertion catches
            // that regression. Per genesis.md Structural Identity §Fee lifecycle.
            let acc_h2 = har.query_contract_state(
                *NATIVE_TOKEN_CONTRACT_ID, "info", b"fee_commit_acc",
            ).expect("sled query").expect("key exists");
            let pt_h2: pallas::Point = Option::from(
                pallas::Point::from_bytes(&acc_h2[..32].try_into().unwrap())
            ).expect("valid point");
            assert_eq!(pt_h2, pallas::Point::identity(),
                "AC-FEE-4: accumulator must be Identity after zero-fee block 2 \
                 — if non-Identity, apply_pow_reward discarded stranded fees");

            let b2 = har.chain_state.get_block(BlockHeight::new(2)).expect("block 2 retrievable");
            assert_eq!(
                b2.header.previous.as_bytes(),
                gen_hash.as_bytes(),
                "AC4: hash chain — block2.previous == hash(genesis)"
            );
            assert_eq!(b2.header.total_reward, reward, "reward correctness");
            assert_eq!(b2.transactions.len(), 1, "one tx in block");
            assert_eq!(
                b2.transactions[0].contract_calls.len(),
                1,
                "one contract call"
            );
            assert!(
                b2.transactions[0].contract_calls[0].contract_id
                    == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                "coinbase targets native_token"
            );

            let sc2 = har.chain_state.supply_chain.get(BlockHeight::new(2))
                .expect("supply_chain at height 2");
            let expected_supply = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1)).get()
                + dwow_sdk::blockchain::expected_reward(BlockHeight::new(2)).get();
            assert_eq!(
                sc2.total_supply, SupplyAmount::new(expected_supply),
                "AC5: supply bridge — S_2 = S_1 + C_2"
            );

            // AC6: ZK proof — coinbase must carry a Mint_V1 proof
            assert!(
                !b2.transactions[0].contract_calls[0].data.is_empty(),
                "AC6: coinbase contract call has data"
            );

            // AC7: Supply chain non-identity — S_2 commitment must not be identity
            let supply_entry = har.chain_state.supply_chain.get(BlockHeight::new(2))
                .expect("supply_chain entry at height 2");
            let identity = pallas::Point::identity();
            assert!(
                supply_entry.value_commit != identity,
                "AC7: cumulative supply commitment S_2 != identity"
            );

            // AC8: Nullifier uniqueness — genesis nullifier ≠ block 2 nullifier.
            // ContractCall doesn't derive PartialEq — compare the call data.
            let b1 = har.chain_state.get_block(BlockHeight::new(1)).expect("block 1 retrievable");
            let empty = vec![];
            let gen_data = b1.transactions[0].contract_calls.first()
                .map(|c| &c.data).unwrap_or(&empty);
            let b2_data = b2.transactions[0].contract_calls.first()
                .map(|c| &c.data).unwrap_or(&empty);
            assert!(
                gen_data != b2_data,
                "AC8: genesis and block 2 coinbase data differ (distinct nullifiers)"
            );

            drop(mgr);
            let _ = std::fs::remove_file(&path);
        });
    }

    /// Zero-fee block: genesis → height-2 coinbase-only block (zero FeeV2
    /// calls, zero FeeCollectV1) through accept_block.
    ///
    /// Proves the zero-fee path is valid per fee-spec §4.4: FeeCollectV1
    /// SHALL be absent when total_fees == 0. Verifies the accumulator stays
    /// Identity across zero-fee blocks. Per genesis.md Structural Identity
    /// §Transaction ordering and §Fee lifecycle.
    #[test]
    fn test_zero_fee_block_accepted() {
        dwow_native_token_contract::enable_deterministic_zk();

        smol::block_on(async {
            let har = GenesisHarness::new_without_contracts().expect("GenesisHarness");

            let keys_toml = "[node0]\nwallet_secret = \
                \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
            let path = std::env::temp_dir()
                .join(format!("dwow_zf_{}.toml", std::process::id()));
            std::fs::write(&path, keys_toml).expect("write test keys");

            let mgr = crate::accounts::AccountManager::open(
                &path,
                dwow_sdk::crypto::keypair::Network::Testnet,
                "node0",
            ).expect("open test AccountManager");
            let recipient =
                crate::accounts::MiningRecipient::from_account(&mgr, BlockHeight::new(1))
                    .expect("MiningRecipient");
            let magic_bytes = [0xDA, 0x57, 0x01, 0x57];

            // Height 1: Genesis
            crate::init_genesis(&har.chain_state, recipient.clone(), magic_bytes)
                .await.expect("init_genesis");
            assert_eq!(har.block_height(), BlockHeight::new(1));

            // Verify accumulator == Identity after genesis
            let acc = har.query_contract_state(
                *NATIVE_TOKEN_CONTRACT_ID, "info", b"fee_commit_acc",
            ).expect("sled query").expect("key exists");
            let pt: pallas::Point = Option::from(
                pallas::Point::from_bytes(&acc[..32].try_into().unwrap())
            ).expect("valid point");
            assert_eq!(pt, pallas::Point::identity(),
                "accumulator must be Identity after genesis (zero-fee block)");

            // Height 2: coinbase-only block (zero FeeV2, zero FeeCollectV1)
            let height = BlockHeight::new(2);
            let reward = dwow_sdk::blockchain::expected_reward(height);
            let linear_zk =
                crate::registry::model::LinearPowRewardZk::new(har.chain_state.clone())
                    .await.expect("LinearPowRewardZk");
            let (_coinbase, _public_inputs, pow_reward_call, _coin_blind) =
                crate::registry::model::build_linear_coinbase(
                    recipient, reward, &linear_zk, height,
                ).await.expect("coinbase for height 2");

            let tx = dwow_chain::Transaction {
                version: BlockVersion::CURRENT,
                inputs: vec![],
                outputs: vec![],
                contract_calls: vec![pow_reward_call],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            };
            let merkle_root = tx.hash();
            let gen_block = har.chain_state.get_block(BlockHeight::new(1))
                .expect("block 1");
            let gen_hash = har.chain_state.hash_block_with_cached_vm(&gen_block).expect("hash failed");

            let header = dwow_chain::BlockHeader {
                version: BlockVersion::CURRENT,
                previous: gen_hash,
                merkle_root,
                timestamp: BlockTimestamp::new(120),
                target: BlockTarget::MAX,
                nonce: 0,
                height,
                uncle_merkle_root: [0u8; 32],
                total_reward: reward,
                randomx_key: dwow_chain::Miner::derive_key_from_height(height),
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: dwow_chain::PowSource::Native,
            };

            let block = dwow_chain::Block { header, transactions: vec![tx] };

            let rx_flags = randomx::RandomXFlags::get_recommended_flags()
                & !randomx::RandomXFlags::JIT;
            let rx_cache = randomx::RandomXCache::new(
                rx_flags, &block.header.randomx_key,
            ).expect("RandomX cache");
            let vm = std::sync::Arc::new(
                randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
                    .expect("RandomX VM"),
            );

            // Submit through accept_block — must succeed (zero-fee path)
            crate::block_acceptor::accept_block(
                &har.chain_state, &block, &[], &vm,
                BlockHeight::new(1), BlockTarget::MAX, None,
            ).expect("AC-ZF-1: zero-fee block must be accepted");

            assert_eq!(har.block_height(), BlockHeight::new(2),
                "AC-ZF-2: height advanced to 2");

            let b2 = har.chain_state.get_block(BlockHeight::new(2))
                .expect("block 2 retrievable");
            assert_eq!(b2.transactions.len(), 1,
                "AC-ZF-3: zero-fee block must have exactly 1 tx (coinbase only)");
            // Verify no FeeCollectV1 in the zero-fee block
            for (i, tx) in b2.transactions.iter().enumerate() {
                for call in &tx.contract_calls {
                    assert_ne!(call.data.first(), Some(&0x06),
                        "AC-ZF-4: tx[{}] must not contain FeeCollectV1 (0x06) \
                         — zero fees", i);
                }
            }

            // Accumulator must still be Identity after zero-fee block
            let acc2 = har.query_contract_state(
                *NATIVE_TOKEN_CONTRACT_ID, "info", b"fee_commit_acc",
            ).expect("sled query").expect("key exists");
            let pt2: pallas::Point = Option::from(
                pallas::Point::from_bytes(&acc2[..32].try_into().unwrap())
            ).expect("valid point");
            assert_eq!(pt2, pallas::Point::identity(),
                "AC-ZF-5: accumulator must be Identity after zero-fee block");

            drop(mgr);
            let _ = std::fs::remove_file(&path);
        });
    }

    /// Build a genesis block on a fresh harness via the production
    /// `init_genesis` path and return (harness, genesis_block, hash).
    async fn build_genesis() -> (GenesisHarness, dwow_chain::Block, blake3::Hash) {
        let har = GenesisHarness::new_without_contracts().expect("GenesisHarness");
        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        static SYNC_COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = SYNC_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("dwow_gen_sync_{}_{}.toml", std::process::id(), n));
        std::fs::write(&path, keys_toml).expect("write test keys");
        let mgr = crate::accounts::AccountManager::open(
            &path, dwow_sdk::crypto::keypair::Network::Testnet, "node0",
        ).expect("open test AccountManager");
        let recipient = crate::accounts::MiningRecipient::from_account(&mgr, BlockHeight::new(1))
            .expect("MiningRecipient");
        drop(mgr);
        let _ = std::fs::remove_file(&path);

        let magic_bytes = [0xDA, 0x57, 0x01, 0x57];
        crate::init_genesis(&har.chain_state, recipient, magic_bytes)
            .await.expect("init_genesis");
        let block = har.chain_state.get_block(BlockHeight::new(1)).expect("block 1");
        let hash = har.chain_state.hash_block_with_cached_vm(&block).expect("hash failed");
        (har, block, hash)
    }

    /// RandomX VM keyed for the genesis block — same construction as
    /// init_genesis and the sync path.
    fn genesis_vm(block: &dwow_chain::Block) -> std::sync::Arc<randomx::RandomXVM> {
        let rx_flags = randomx::RandomXFlags::get_recommended_flags()
            & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(rx_flags, &block.header.randomx_key)
            .expect("RandomX cache");
        std::sync::Arc::new(
            randomx::RandomXVM::new(rx_flags, Some(rx_cache), None).expect("RandomX VM"),
        )
    }

    /// Witness (a): a syncing node with an EMPTY contracts tree accepts the
    /// genesis block through the exact sync-path call shape
    /// (consensus_linear.rs accept_block) and ends with all 9 contracts +
    /// manifests materialized and the identical block hash. This is the
    /// one-or-the-other invariant: the syncing node deployed NOTHING at
    /// startup — the genesis block provided everything.
    #[test]
    fn test_genesis_sync_materializes_contracts() {
        dwow_native_token_contract::enable_deterministic_zk();

        smol::block_on(async {
            let (_har_a, genesis_block, hash_a) = build_genesis().await;

            // Sizing witness: the genesis block (WASM in serde_json byte
            // arrays) exceeds the non-genesis cap — the height-1 size
            // exemption in accept_block is load-bearing.
            let genesis_len = serde_json::to_vec(&genesis_block).expect("serialize").len();
            assert!(genesis_len > dwow_chain::execution::MAX_BLOCK_SIZE,
                "genesis block {} bytes should exceed MAX_BLOCK_SIZE {} — \
                 if not, the size exemption is dead code",
                genesis_len, dwow_chain::execution::MAX_BLOCK_SIZE);

            // Harness B: the syncing node — empty contracts tree.
            let har_b = GenesisHarness::new_without_contracts().expect("GenesisHarness B");
            for (cid, name) in dwow_chain::execution::genesis_contracts() {
                let stored = har_b.chain_state.store
                    .get_contract_data(&cid.to_bytes())
                    .expect("get_contract_data");
                assert!(stored.is_empty(),
                    "pre-sync: {name} must NOT exist on the syncing node");
            }

            // Exact sync-path call shape (consensus_linear.rs).
            let vm = genesis_vm(&genesis_block);
            crate::block_acceptor::accept_block(
                &har_b.chain_state,
                &genesis_block,
                &[],
                &vm,
                BlockHeight::new(0),
                BlockTarget::MAX,
                None,
            ).expect("sync accept_block(genesis) must succeed on empty node");

            assert_eq!(har_b.block_height(), BlockHeight::new(1), "synced to height 1");

            // All 9 contracts + manifests materialized from the block.
            for (i, (cid, name)) in
                dwow_chain::execution::genesis_contracts().iter().enumerate()
            {
                let params: dwow_sdk::deploy::DeployParamsV1 = dwow_serial::deserialize(
                    &genesis_block.transactions[i + 1].contract_calls[0].data[1..],
                ).expect("DeployParamsV1 decode");
                let stored = har_b.chain_state.store
                    .get_contract_data(&cid.to_bytes())
                    .expect("get_contract_data");
                assert_eq!(stored, params.wasm_bincode,
                    "post-sync: {name} WASM byte-equal to genesis payload");
                let mut mkey = Vec::from(cid.to_bytes().as_slice());
                mkey.extend_from_slice(b"_manifest");
                let manifest = har_b.chain_state.store
                    .get_contract_data(&mkey)
                    .expect("get manifest");
                assert_eq!(manifest, params.ix,
                    "post-sync: {name} manifest matches genesis payload");
            }

            // Supply chain seeded by the coinbase execution.
            let sc = har_b.chain_state.supply_chain.get(BlockHeight::new(1))
                .expect("supply_chain at height 1");
            assert_eq!(sc.total_supply,
                SupplyAmount::new(dwow_sdk::blockchain::expected_reward(BlockHeight::new(1)).get()),
                "synced node: S_1 == INITIAL_REWARD");

            // Identical chain identity.
            let hash_b = har_b.chain_state.hash_block_with_cached_vm(&genesis_block).expect("hash failed");
            assert_eq!(hash_a, hash_b, "genesis hash identical on creator and syncer");
        });
    }

    /// Witness (b): tampering with the genesis deployment payload is
    /// rejected. (i) A flipped WASM byte without recomputing the merkle
    /// root fails check_block_header. (ii) With the merkle recomputed the
    /// block hash differs — the genesis-hash pin / Tip filter rejects it
    /// network-wide. (iii) Swapped deployment order (merkle recomputed)
    /// fails the apply_genesis_deployments position binding.
    #[test]
    fn test_genesis_tampered_wasm_rejected() {
        dwow_native_token_contract::enable_deterministic_zk();

        smol::block_on(async {
            let (_har_a, genesis_block, hash_a) = build_genesis().await;

            // (i) Flip one byte inside deployment 1's WASM payload, merkle
            // root left stale → merkle mismatch rejects the block.
            let mut tampered = genesis_block.clone();
            let data = &mut tampered.transactions[1].contract_calls[0].data;
            let mid = data.len() / 2;
            data[mid] ^= 0xFF;
            let har_i = GenesisHarness::new_without_contracts().expect("harness i");
            let vm = genesis_vm(&tampered);
            let res = crate::block_acceptor::accept_block(
                &har_i.chain_state, &tampered, &[], &vm,
                BlockHeight::new(0), BlockTarget::MAX, None,
            );
            assert!(res.is_err(),
                "(i) tampered WASM with stale merkle root MUST be rejected");

            // (ii) Recompute the merkle root — the block hash now differs
            // from the authority's, which is exactly what the genesis-hash
            // pin and the Tip genesis_hash chain-compat filter reject.
            tampered.header.merkle_root =
                dwow_chain::compute_merkle_root(&tampered.transactions);
            let har_ii = GenesisHarness::new_without_contracts().expect("harness ii");
            let hash_tampered =
                har_ii.chain_state.hash_block_with_cached_vm(&tampered).expect("hash failed");
            assert_ne!(hash_a, hash_tampered,
                "(ii) tampered genesis has a different hash — pin rejects it");

            // (iii) Swap two deployment txs (Deployooor <-> NativeToken),
            // merkle recomputed → position binding (singleton_name order)
            // fails in apply_genesis_deployments.
            let mut swapped = genesis_block.clone();
            swapped.transactions.swap(1, 2);
            swapped.header.merkle_root =
                dwow_chain::compute_merkle_root(&swapped.transactions);
            let har_iii = GenesisHarness::new_without_contracts().expect("harness iii");
            let vm = genesis_vm(&swapped);
            let res = crate::block_acceptor::accept_block(
                &har_iii.chain_state, &swapped, &[], &vm,
                BlockHeight::new(0), BlockTarget::MAX, None,
            );
            assert!(res.is_err(),
                "(iii) out-of-order deployments MUST fail position binding");
        });
    }

    /// T1: Dual-node block persistence — two independent sleb stores MUST
    /// converge when the same blocks are inserted into both via LinearStore.
    ///
    /// This is the persistence-boundary equivalent of sync: the receiving node
    /// applies blocks by inserting them into its store, then re-lifts them via
    /// from_le_bytes at the same height key (§2.3). Both stores MUST return
    /// identical blocks after the same set of insertions.
    ///
    /// Lightweight — pure sled I/O, no CChainState, no WASM, no ZK, no RandomX.
    #[test]
    fn test_dual_node_persistence_roundtrip() {
        use dwow_chain::LinearStore;

        let store1 = LinearStore::new(
            Arc::new(sled::Config::new().temporary(true).open().unwrap())
        ).unwrap();
        let store2 = LinearStore::new(
            Arc::new(sled::Config::new().temporary(true).open().unwrap())
        ).unwrap();

        for h in 1u64..=5 {
            let height = BlockHeight::new(h);
            let block = dwow_chain::Block {
                header: dwow_chain::BlockHeader {
                    version: BlockVersion::CURRENT,
                    previous: if h == 1 { blake3::hash(b"genesis") } else { blake3::hash(&h.to_le_bytes()) },
                    merkle_root: dwow_chain::compute_merkle_root(&[]),
                    timestamp: BlockTimestamp::new(h),
                    target: BlockTarget::MAX,
                    nonce: h as u32,
                    height,
                    uncle_merkle_root: [0u8; 32],
                    total_reward: BlockReward::ZERO,
                    randomx_key: dwow_chain::Miner::derive_key_from_height(height),
                    coin_merkle_root: [0u8; 32],
                    nullifier_root: [0u8; 32],
                    anchor_tx_id: [0u8; 32],
                    anchor_monero_height: MoneroBlockHeight::new(0),
                    anchor_monero_hash: [0u8; 32],
                    finality_flags: 0,
                    pow_source: dwow_chain::PowSource::Native,
                },
                transactions: vec![],
            };
            store1.insert_block(height, &block).expect("store1 insert");
            store2.insert_block(height, &block).expect("store2 insert");
        }

        // Both stores MUST converge — same blocks at same heights.
        for h in 1u64..=5 {
            let b1 = store1.get_block(BlockHeight::new(h)).expect("store1 get_block");
            let b2 = store2.get_block(BlockHeight::new(h)).expect("store2 get_block");
            assert_eq!(b1.header.height, b2.header.height);
            assert_eq!(b1.header.previous, b2.header.previous,
                "divergent previous hash at h={} — sync persistence violated", h);
        }
    }
}
