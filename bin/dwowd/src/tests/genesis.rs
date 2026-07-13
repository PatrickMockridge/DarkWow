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
    /// Create a new GenesisHarness with temp sled DB and CChainState.
    /// Deploys NativeToken and Deployooor WASM so the chain is ready
    /// for contract deployment.
    pub fn new() -> Result<Self> {
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
            initial_target: u32::MAX, // matches test block u32::MAX target
            min_target: 1,
            max_target: u32::MAX,
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

        Ok(Self { db, chain_state })
    }

    /// Store a WASM contract in the chain state so it can be looked up.
    pub fn deploy_contract(&self, wasm: &[u8], contract_id: ContractId) -> Result<()> {
        self.chain_state.store.set_contract_data(&contract_id.to_bytes(), wasm)
            .map_err(|e| dwow_core::Error::Custom(format!("Failed to store contract WASM: {}", e)))
    }

    /// Get current block height.
    pub fn block_height(&self) -> u64 {
        self.chain_state.get_height()
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
            let har1 = GenesisHarness::new().expect("GenesisHarness 1");
            let har2 = GenesisHarness::new().expect("GenesisHarness 2");

            // Initialize contracts on both harnesses
            crate::init_genesis_contracts(&har1.chain_state)
                .expect("init_genesis_contracts har1: all 9 contracts must init");
            crate::init_genesis_contracts(&har2.chain_state)
                .expect("init_genesis_contracts har2: all 9 contracts must init");

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
            let recipient1 = crate::accounts::MiningRecipient::from_account(&mgr, 1)
                .expect("MiningRecipient 1");
            let recipient2 = crate::accounts::MiningRecipient::from_account(&mgr, 1)
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
            assert_eq!(har1.block_height(), 1);
            assert_eq!(har2.block_height(), 1);

            let block1 = har1.chain_state.get_block(1).expect("har1 block 1");
            let block2 = har2.chain_state.get_block(1).expect("har2 block 1");

            // AC5: total_reward == expected_reward(1) = INITIAL_REWARD
            let expected = dwow_sdk::blockchain::expected_reward(1);
            assert_eq!(block1.header.total_reward, expected, "AC5: total_reward");
            assert_eq!(block2.header.total_reward, expected, "AC5: total_reward");

            // AC6: previous == [0u8; 32]
            assert_eq!(block1.header.previous.as_bytes(), &[0u8; 32], "AC6: previous");
            assert_eq!(block2.header.previous.as_bytes(), &[0u8; 32], "AC6: previous");

            // AC7: timestamp == 0
            assert_eq!(block1.header.timestamp, 0, "AC7: timestamp");
            assert_eq!(block2.header.timestamp, 0, "AC7: timestamp");

            // AC8: nonce == 0
            assert_eq!(block1.header.nonce, 0, "AC8: nonce");
            assert_eq!(block2.header.nonce, 0, "AC8: nonce");

            // AC9: target == u32::MAX
            assert_eq!(block1.header.target, u32::MAX, "AC9: target");
            assert_eq!(block2.header.target, u32::MAX, "AC9: target");
        });
    }
}
