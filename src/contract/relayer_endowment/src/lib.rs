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

//! DarkFi Relayer Endowment Contract
//!
//! ## Overview
//!
//! External capital providers ("backers") deploy capital to relayers in exchange
//! for a share of the relayer's fees. This enables relayers to operate with
//! more coverage than their own stake alone.
//!
//! ## How It Works
//!
//! 1. **Initialize**: A relayer initializes an endowment account
//! 2. **Deploy Capital**: Backer deploys DAI/NETHER to the relayer's endowment
//! 3. **Earn Fees**: Backer receives a percentage of relayer's bridge fees
//! 4. **Withdraw**: Backer can withdraw their deployment + accumulated fees
//!
//! ## Economic Model
//!
//! - **Deployment**: Backer locks capital with a relayer
//! - **Fee Share**: Backer receives `backer_cut_bp` of relayer's earned fees
//! - **Yield**: Over time, the backer's share of fees provides yield
//! - **Withdrawal**: Backer can withdraw their principal + earnings
//!
//! ## Composability
//!
//! This contract composes patterns from dao_escrow for endowment management
//! and betting_stake for proportional share calculations.

use darkfi_sdk::{
    error::ContractError,
};

/// Relayer Endowment Functions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RelayerEndowmentFunction {
    /// Initialize endowment account for a relayer
    InitializeV1 = 0x00,
    /// Backer deploys capital to a relayer's endowment
    DeployCapitalV1 = 0x01,
    /// Backer withdraws their deployment
    WithdrawDeploymentV1 = 0x02,
    /// Backer claims their share of relayer fees
    ClaimRelayerFeesV1 = 0x03,
    /// Relayer settles fees to backers
    SettleFeesV1 = 0x04,
    /// Update fee configuration
    UpdateConfigV1 = 0x05,
}

impl TryFrom<u8> for RelayerEndowmentFunction {
    type Error = ContractError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::DeployCapitalV1),
            0x02 => Ok(Self::WithdrawDeploymentV1),
            0x03 => Ok(Self::ClaimRelayerFeesV1),
            0x04 => Ok(Self::SettleFeesV1),
            0x05 => Ok(Self::UpdateConfigV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Internal contract errors
pub mod error;

/// Call parameters definitions
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

// Database tree names
/// Endowment registry tree - stores relayer endowment accounts
pub const RELAYER_ENDOWMENT_REGISTRY_TREE: &str = "endowment_registry";
/// Deployment tree - stores individual deployments
pub const RELAYER_ENDOWMENT_DEPLOYMENTS_TREE: &str = "endowment_deployments";
/// Accumulated fees tree - stores fee allocations per deployment
pub const RELAYER_ENDOWMENT_FEES_TREE: &str = "endowment_fees";
/// Info tree - stores contract info (version, config)
pub const RELAYER_ENDOWMENT_INFO_TREE: &str = "relayer_endowment_info";

// Database keys
/// Database version key
pub const RELAYER_ENDOWMENT_DB_VERSION: &[u8] = b"db_version";

// Constants
/// Minimum deployment amount
pub const RELAYER_ENDOWMENT_MIN_DEPLOY: u64 = 1_000_000; // 1 DAI equivalent
/// Basis points precision for fee calculations
pub const RELAYER_ENDOWMENT_BP_PRECISION: u32 = 10000;

// zkas circuit namespaces
pub const RELAYER_ENDOWMENT_ZKAS_INIT_NS_V1: &str = "Init_V1";
pub const RELAYER_ENDOWMENT_ZKAS_DEPLOY_CAPITAL_NS_V1: &str = "DeployCapital_V1";
pub const RELAYER_ENDOWMENT_ZKAS_CLAIM_FEES_NS_V1: &str = "ClaimFees_V1";