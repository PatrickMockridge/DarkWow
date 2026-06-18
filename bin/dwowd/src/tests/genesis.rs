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
//! Creates a temp sled DB and CChainState with the two mandatory native contracts
//! (NativeToken + Deployooor) pre-deployed. Uses `store.set_contract_data()` to
//! store WASM bytes directly — no block mining needed for test setup.
//!
//! Updated for CChainState (commit 597691582 refactor — replaces LinearBlockchain).

use std::sync::Arc;

use dwow_chain::{CChainState, FinalityConfig, PoWConfig};
use dwow_core::Result;
use dwow_sdk::crypto::{
    ATTESTATION_CONTRACT_ID, ContractId, DEPLOYOOOR_CONTRACT_ID,
    IDENTITY_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID, ORACLE_CONTRACT_ID,
};

/// Reusable baseline chain with NativeToken + Deployooor deployed.
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
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .map_err(|e| dwow_core::Error::Custom(format!("Failed to create temp sled DB: {}", e)))?;
        let db = Arc::new(db);

        let pow_config = PoWConfig::default();
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
