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

//! DrainProtection module - WASM contract interaction
//!
//! This module handles DrainProtection contract interactions:
//! - InitializeV1: Create a new protected fund linked to a DAO-Escrow
//!
//! DrainProtection provides governance for endowment/treasury funds:
//! - Rate limiting per block
//! - 2/3 vote thresholds for large withdrawals
//! - Emergency lock/unlock controls

use darkfi::{tx::{ContractCallLeaf, Transaction}, Error, Result};
use darkfi_sdk::{crypto::PublicKey, pasta::pallas, tx::ContractCall};
use darkfi_serial::Encodable;

use darkfi_drain_protection_contract::model::InitializeParamsV1;
use crate::contract_imports::drain_protection::DrainProtectionFunction;
use crate::fee_builder::build_fee_and_finalize_tx;
use crate::Drk;

use darkfi_drain_protection_contract::model::DrainConfig;

impl Drk {
    /// Initialize a new DrainProtection protected fund
    ///
    /// This creates a DrainProtection instance linked to a DAO-Escrow.
    /// The resulting bulla is used to enable drain protection on the DAO-Escrow.
    ///
    /// # Arguments
    /// * `fund_id` - Fund ID (typically derived from DAO-Escrow bulla)
    /// * `spend_authority` - Public key authorized to propose withdrawals
    /// * `dao_escrow_bulla` - The DAO-Escrow bulla this fund protects
    /// * `rate_limit_bps` - Base rate limit in basis points (e.g., 100 = 1%)
    /// * `vote_threshold_bps` - Vote threshold in basis points (e.g., 667 = 66.7%)
    pub async fn drain_protection_initialize(
        &self,
        fund_id: pallas::Base,
        spend_authority: PublicKey,
        dao_escrow_bulla: pallas::Base,
        rate_limit_bps: u64,
        vote_threshold_bps: u64,
    ) -> Result<Transaction> {
        // Build initialize params using model types
        let params = InitializeParamsV1 {
            fund_id,
            spend_authority,
            dao_escrow_bulla,
            drain_config: DrainConfig {
                graduated_tiers: None,
                exit_queue: None,
                circuit_breaker: None,
                guardian_pause: None,
                observation_period: None,
                split_proposals: None,
                no_loss_reserve: None,
                dead_mans_switch: None,
            },
        };

        // Create function call data
        let function = DrainProtectionFunction::InitializeV1 as u8;
        let mut call_data = vec![function];
        params.encode(&mut call_data)
            .map_err(|e| Error::Custom(format!("Failed to encode params: {:?}", e)))?;

        // Get DrainProtection contract ID
        let drain_protection_id = crate::contract_imports::DRAIN_PROTECTION_CONTRACT_ID.get()
            .copied()
            .ok_or_else(|| Error::Custom("DrainProtection contract ID not initialized".to_string()))?;

        // Create contract call
        let dp_call = ContractCall {
            contract_id: drain_protection_id,
            data: call_data,
        };

        // Create contract call leaf with no proofs (InitializeV1 is simple state update)
        let dp_leaf = ContractCallLeaf { call: dp_call, proofs: vec![] };

        // Build fee and finalize
        let tx = build_fee_and_finalize_tx(&self.wallet, dp_leaf).await?;

        Ok(tx)
    }
}