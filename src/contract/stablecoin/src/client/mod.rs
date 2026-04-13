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

//! Client-side transaction builders for stablecoin contract
//!
//! ## MoneyV3 Integration
//!
//! Stablecoin uses MoneyV3 for token management. When initializing:
//! 1. `InitializeV1` creates a MoneyV3 token type (e.g., "USDx")
//! 2. `OpenPositionV1` mints collateral receipt tokens via MoneyV3
//! 3. `MintStableV1` burns collateral tokens, mints stablecoin via MoneyV3
//! 4. `LiquidateV1` uses spend_hook for seizure callbacks

use crate::model::*;

#[cfg(feature = "client")]
pub use darkfi_money_v3_contract::client::token_mint_v1::TokenMintCallInput;

// ============================================================================
// ZK Proof Generation Modules
// ============================================================================

pub mod initialize_v1;
pub mod open_position_v1;
pub mod mint_stable_v1;
pub mod liquidate_v1;
pub mod governance_report_v1;
pub mod accrue_interest_v1;

// ============================================================================
// Transaction Builders
// ============================================================================

/// Builder for opening a new CDP
pub struct OpenPositionBuilder {
    /// Collateral amount
    pub collateral_amount: u64,
    /// Debt amount (stablecoin to mint)
    pub debt_amount: u64,
    /// Owner's public key
    pub owner_pub_x: [u8; 32],
    pub owner_pub_y: [u8; 32],
    /// Collateral type
    pub collateral_type: CollateralType,
}

impl OpenPositionBuilder {
    /// Create a new position commitment
    pub fn position_commitment(&self, _secret: [u8; 32]) -> [u8; 32] {
        // Commitment is computed via ZK proof in open_position_v1.rs
        [0u8; 32]
    }
}

/// Builder for adding collateral
pub struct AddCollateralBuilder {
    /// Position nullifier
    pub position_nullifier: [u8; 32],
    /// Amount of collateral to add
    pub added_collateral: u64,
}

impl AddCollateralBuilder {
    /// Create new commitment after adding collateral
    pub fn new_commitment(
        &self,
        _current_secret: [u8; 32],
        _current_collateral: u64,
        _current_debt: u64,
        _owner_pub_x: [u8; 32],
        _owner_pub_y: [u8; 32],
    ) -> [u8; 32] {
        // Commitment is computed via ZK proof
        [0u8; 32]
    }
}

/// Builder for removing collateral
pub struct RemoveCollateralBuilder {
    /// Position nullifier
    pub position_nullifier: [u8; 32],
    /// Amount of collateral to remove
    pub removed_collateral: u64,
}

/// Builder for minting stablecoin
pub struct MintStableBuilder {
    /// Position nullifier
    pub position_nullifier: [u8; 32],
    /// Amount of stablecoin to mint
    pub mint_amount: u64,
}

/// Builder for repaying stablecoin debt
pub struct RepayStableBuilder {
    /// Position nullifier
    pub position_nullifier: [u8; 32],
    /// Amount of stablecoin to repay
    pub repay_amount: u64,
}

/// Builder for liquidating a CDP
pub struct LiquidateBuilder {
    /// Position nullifier to liquidate
    pub position_nullifier: [u8; 32],
    /// Current collateral amount
    pub collateral_amount: u64,
    /// Current debt amount
    pub debt_amount: u64,
    /// Current TWAP price
    pub current_price: u64,
}

// ============================================================================