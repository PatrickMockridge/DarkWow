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

//! DarkFi Stablecoin Contract (CDP)
//!
//! A privacy-preserving collateralized debt position (CDP) stablecoin
//! inspired by Nethermind's P2P oracle design. Key features:
//!
//! - **AMM-based price feed**: TWAP from NETHER/DRK pool, no trusted oracles
//! - **PI Controller**: Algorithmic redemption rate adjustment for stability
//! - **Privacy by default**: All positions, amounts, and identities hidden via ZK
//! - **Pedersen commitments**: Collateral and debt amounts hidden in SMT
//! - **CDP Notes**: Special Money contract coins with spend_hook to CDP Engine

use darkfi_sdk::error::ContractError;

/// Functions available in the stablecoin contract
#[repr(u8)]
#[derive(Debug)]
pub enum StablecoinFunction {
    /// Initialize the CDP engine with initial parameters
    InitializeV1 = 0x00,
    /// Open a new collateralized debt position
    OpenPositionV1 = 0x01,
    /// Add collateral to an existing position
    AddCollateralV1 = 0x02,
    /// Remove collateral from a position
    RemoveCollateralV1 = 0x03,
    /// Mint stablecoin against collateral
    MintStableV1 = 0x04,
    /// Repay stablecoin debt
    RepayStableV1 = 0x05,
    /// Liquidate an undercollateralized position
    LiquidateV1 = 0x06,
    /// Update CDP engine parameters (PI controller, rates)
    UpdateConfigV1 = 0x07,
}

impl TryFrom<u8> for StablecoinFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::OpenPositionV1),
            0x02 => Ok(Self::AddCollateralV1),
            0x03 => Ok(Self::RemoveCollateralV1),
            0x04 => Ok(Self::MintStableV1),
            0x05 => Ok(Self::RepayStableV1),
            0x06 => Ok(Self::LiquidateV1),
            0x07 => Ok(Self::UpdateConfigV1),
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

// ============================================================================
// CDP (Collateralized Debt Position) Constants
// ============================================================================

/// Minimum collateralization ratio (e.g., 150% = 15000 basis points)
pub const CDP_MIN_COLLATERALIZATION_RATIO: u64 = 15000;
/// Liquidation threshold ratio (e.g., 130% = 13000 basis points)
pub const CDP_LIQUIDATION_THRESHOLD: u64 = 13000;
/// Liquidation penalty percentage (e.g., 10% = 1000 basis points)
pub const CDP_LIQUIDATION_PENALTY: u64 = 1000;

/// Base rate for stability fee (annual rate in basis points)
pub const CDP_BASE_RATE: u64 = 500;

/// PI Controller constants
/// Proportional gain
pub const CDP_PI_KP: i64 = 1000;
/// Integral gain
pub const CDP_PI_KI: i64 = 100;

/// Price feed TWAP window in seconds (e.g., 1 hour)
pub const CDP_PRICE_FEED_TWAP_WINDOW: u64 = 3600;

/// Target price deviation threshold for PI controller adjustment
pub const CDP_PRICE_DEVIATION_THRESHOLD: u64 = 500; // 5%

// ============================================================================
// Database Tree Names
// ============================================================================

// These are the sled trees that will be created
pub const STABLECOIN_CONTRACT_INFO_TREE: &str = "info";
pub const STABLECOIN_CONTRACT_POSITIONS_TREE: &str = "positions";
pub const STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE: &str = "position_nullifiers";
pub const STABLECOIN_CONTRACT_STABLECOIN_TREE: &str = "stablecoin";
pub const STABLECOIN_CONTRACT_COLLATERAL_TREE: &str = "collateral";
pub const STABLECOIN_CONTRACT_LIQUIDATIONS_TREE: &str = "liquidations";

// Keys inside the info tree
pub const STABLECOIN_CONTRACT_DB_VERSION: &[u8] = b"db_version";
pub const STABLECOIN_CONTRACT_POSITION_TREE: &[u8] = b"positions_tree";
pub const STABLECOIN_CONTRACT_LATEST_POSITION_ROOT: &[u8] = b"last_position_root";
pub const STABLECOIN_CONTRACT_STABLECOIN_SUPPLY: &[u8] = b"stablecoin_supply";
pub const STABLECOIN_CONTRACT_PI_CONTROLLER_STATE: &[u8] = b"pi_controller_state";
pub const STABLECOIN_CONTRACT_REDEMPTION_RATE: &[u8] = b"redemption_rate";

// ============================================================================
// zkas Circuit Namespaces
// ============================================================================

/// zkas open position circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_OPEN_NS_V1: &str = "OpenPosition_V1";
/// zkas add collateral circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_ADD_COLLATERAL_NS_V1: &str = "AddCollateral_V1";
/// zkas remove collateral circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_REMOVE_COLLATERAL_NS_V1: &str = "RemoveCollateral_V1";
/// zkas mint stablecoin circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_MINT_STABLE_NS_V1: &str = "MintStable_V1";
/// zkas repay stablecoin circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_REPAY_STABLE_NS_V1: &str = "RepayStable_V1";
/// zkas liquidate circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_LIQUIDATE_NS_V1: &str = "Liquidate_V1";