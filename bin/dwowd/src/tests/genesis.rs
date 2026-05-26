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
//! Creates a temp sled DB, LinearStore, and LinearBlockchain with the two
//! mandatory native contracts (NativeToken + Deployooor) pre-deployed.
//! No block mining — `deploy_contract()` stores WASM directly in LinearStore.

use std::sync::Arc;

use dwow_core::Result;
use dwow_chain::{FinalityConfig, LinearStore};
use dwow_sdk::crypto::{
    ContractId, DEPLOYOOOR_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID,
};

use crate::blockchain::{LinearBlockchain, LinearPoWConfig};

/// Reusable baseline chain with NativeToken + Deployooor deployed.
pub struct GenesisHarness {
    /// Temp sled database
    pub db: Arc<sled::Db>,
    /// Linear storage backend
    pub store: Arc<LinearStore>,
    /// Linear blockchain with WASM runtime
    pub blockchain: LinearBlockchain,
}

impl GenesisHarness {
    /// Create a new GenesisHarness with temp sled DB, LinearStore, and
    /// LinearBlockchain. Deploys NativeToken and Deployooor WASM so the
    /// chain is ready for contract deployment.
    pub fn new() -> Result<Self> {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .map_err(|e| dwow_core::Error::Custom(format!("Failed to create temp sled DB: {}", e)))?;
        let db = Arc::new(db);

        let store = LinearStore::new(db.clone())
            .map_err(|e| dwow_core::Error::Custom(format!("Failed to create LinearStore: {}", e)))?;
        let store = Arc::new(store);

        // Use max target so any nonce passes PoW — instant blocks for tests.
        // The production path (stratum, mm_rpc, local miner) is the identical
        // `apply_block_with_uncles()` call — only the target differs.
        let pow_config = LinearPoWConfig {
            target_block_time: 120,
            initial_target: u32::MAX,
            min_target: 1,
            max_target: u32::MAX,
        };
        let finality_config = FinalityConfig::default();
        let blockchain =
            LinearBlockchain::with_pow_config(store.clone(), pow_config, finality_config);

        let deployooor_wasm = include_bytes!(
            "../../../../src/contract/deployooor/dwow_deployooor_contract.wasm"
        );
        blockchain.deploy_contract(deployooor_wasm, *DEPLOYOOOR_CONTRACT_ID, &[])?;

        let native_token_wasm = include_bytes!(
            "../../../../src/contract/native_token/dwow_native_token_contract.wasm"
        );
        blockchain.deploy_contract(native_token_wasm, *NATIVE_TOKEN_CONTRACT_ID, &[])?;

        Ok(Self { db, store, blockchain })
    }

    /// Deploy a WASM contract to the chain.
    pub fn deploy_contract(&self, wasm: &[u8], contract_id: ContractId, ix: &[u8]) -> Result<()> {
        self.blockchain.deploy_contract(wasm, contract_id, ix)
    }

    /// Get current block height.
    pub fn block_height(&self) -> u64 {
        self.blockchain.get_height()
    }
}
