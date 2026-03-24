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

use darkfi_sdk::{brute_force, contract::ContractCall, runtime::Runtime};

use crate::model::*;

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
    pub fn position_commitment(&self, secret: [u8; 32]) -> [u8; 32] {
        // commitment = H(secret, collateral, debt, owner_pub)
        brute_force::poseidon_hash([secret, self.collateral_amount as u8, self.debt_amount as u8])
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
        current_secret: [u8; 32],
        current_collateral: u64,
        current_debt: u64,
        owner_pub_x: [u8; 32],
        owner_pub_y: [u8; 32],
    ) -> [u8; 32] {
        let new_collateral = current_collateral + self.added_collateral;
        brute_force::poseidon_hash([
            current_secret,
            new_collateral as u8,
            current_debt as u8,
            owner_pub_x,
            owner_pub_y,
        ])
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
// TODO: Implement ZK proof generation for each operation
// ============================================================================
//
// Each builder needs to produce a ZK proof demonstrating:
//
// OpenPosition:
//   - commitment = H(secret, collateral, debt, owner_pub)
//   - collateral >= minimum_deposit
//   - debt <= collateral * min_ratio
//   - Merkle proof of commitment insertion
//
// AddCollateral:
//   - Position exists via nullifier
//   - new_commitment = H(secret, old_collateral + added, debt, owner)
//   - added > 0
//
// RemoveCollateral:
//   - Position exists via nullifier
//   - new_commitment = H(secret, old_collateral - removed, debt, owner)
//   - (old_collateral - removed) / debt >= min_ratio
//
// MintStable:
//   - Position exists via nullifier
//   - new_commitment = H(secret, collateral, debt + mint, owner)
//   - (collateral * price) / (debt + mint) >= min_ratio
//
// RepayStable:
//   - Position exists via nullifier
//   - new_commitment = H(secret, collateral, debt - repay, owner)
//   - repay <= debt
//   - Stablecoin burn proof
//
// Liquidate:
//   - Position exists via nullifier
//   - (collateral * price) / debt < liquidation_threshold
//   - Current TWAP from AMM pool
//
// ============================================================================