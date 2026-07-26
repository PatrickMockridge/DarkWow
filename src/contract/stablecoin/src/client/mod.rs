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

//! Client-side transaction builders for stablecoin contract
//!
//! ## PromissoryNote Integration
//!
//! Stablecoin uses PromissoryNote for token management. When initializing:
//! 1. `InitializeV1` creates a PromissoryNote token type (e.g., "USDx")
//! 2. `OpenPositionV1` mints collateral receipt tokens via PromissoryNote
//! 3. `MintStableV1` burns collateral tokens, mints stablecoin via PromissoryNote
//! 4. `LiquidateV1` uses spend_hook for seizure callbacks

use crate::model::*;

#[cfg(feature = "client")]
pub use dwow_promissory_note_contract::client::token_mint_v1::TokenMintCallInput;

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
///
/// ## Usage
///
/// ```ignore
/// let builder = OpenPositionBuilder::new()
///     .owner_secret(secret)
///     .collateral_amount(1000)
///     .debt_amount(500)
///     .collateral_type(CollateralType::Xmr);
/// let call_data = builder.build()?;
/// ```
pub struct OpenPositionBuilder {
    /// Owner's secret key
    owner_secret: Option<pallas::Base>,
    /// Collateral amount
    collateral_amount: Option<u64>,
    /// Debt amount (stablecoin to mint)
    debt_amount: Option<u64>,
    /// Collateral type
    collateral_type: Option<CollateralType>,
}

impl OpenPositionBuilder {
    /// Create a new empty builder
    pub fn new() -> Self {
        Self {
            owner_secret: None,
            collateral_amount: None,
            debt_amount: None,
            collateral_type: None,
        }
    }

    /// Set the owner's secret key
    pub fn owner_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.owner_secret = Some(secret);
        self
    }

    /// Set the collateral amount
    pub fn collateral_amount(&mut self, amount: u64) -> &mut Self {
        self.collateral_amount = Some(amount);
        self
    }

    /// Set the debt amount (stablecoin to mint)
    pub fn debt_amount(&mut self, amount: u64) -> &mut Self {
        self.debt_amount = Some(amount);
        self
    }

    /// Set the collateral type
    pub fn collateral_type(&mut self, ct: CollateralType) -> &mut Self {
        self.collateral_type = Some(ct);
        self
    }

    /// Build the call data for OpenPosition
    ///
    /// Returns the call data and public inputs for ZK proof generation.
    /// The public inputs are used by the host to verify the proof.
    pub fn build(&self) -> Result<OpenPositionCallData, StablecoinClientError> {
        let owner_secret = self.owner_secret.ok_or_else(|| StablecoinClientError::MissingField("owner_secret"))?;
        let collateral_amount = self.collateral_amount.ok_or_else(|| StablecoinClientError::MissingField("collateral_amount"))?;
        let debt_amount = self.debt_amount.ok_or_else(|| StablecoinClientError::MissingField("debt_amount"))?;
        let collateral_type = self.collateral_type.clone().ok_or_else(|| StablecoinClientError::MissingField("collateral_type"))?;

        // Convert collateral type to pallas::Base
        let ct_base = match collateral_type {
            CollateralType::Xmr => pallas::Base::zero(),
            CollateralType::Drk => pallas::Base::one(),
            CollateralType::Eth => pallas::Base::from(2),
        };

        Ok(OpenPositionCallData::new(
            owner_secret,
            collateral_amount,
            debt_amount,
            ct_base,
        ))
    }
}

impl Default for OpenPositionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for adding collateral to an existing position
pub struct AddCollateralBuilder {
    /// Position nullifier (to prove ownership)
    position_nullifier: Option<pallas::Base>,
    /// Amount of collateral to add
    added_collateral: Option<u64>,
    /// Owner's secret key
    owner_secret: Option<pallas::Base>,
    /// Current collateral amount
    current_collateral: Option<u64>,
    /// Current debt amount
    current_debt: Option<u64>,
    /// Collateral type
    collateral_type: Option<pallas::Base>,
}

impl AddCollateralBuilder {
    /// Create a new empty builder
    pub fn new() -> Self {
        Self {
            position_nullifier: None,
            added_collateral: None,
            owner_secret: None,
            current_collateral: None,
            current_debt: None,
            collateral_type: None,
        }
    }

    /// Set the position nullifier
    pub fn position_nullifier(&mut self, nullifier: pallas::Base) -> &mut Self {
        self.position_nullifier = Some(nullifier);
        self
    }

    /// Set the amount of collateral to add
    pub fn added_collateral(&mut self, amount: u64) -> &mut Self {
        self.added_collateral = Some(amount);
        self
    }

    /// Set the owner's secret key
    pub fn owner_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.owner_secret = Some(secret);
        self
    }

    /// Set the current collateral amount
    pub fn current_collateral(&mut self, amount: u64) -> &mut Self {
        self.current_collateral = Some(amount);
        self
    }

    /// Set the current debt amount
    pub fn current_debt(&mut self, amount: u64) -> &mut Self {
        self.current_debt = Some(amount);
        self
    }

    /// Set the collateral type
    pub fn collateral_type(&mut self, ct: pallas::Base) -> &mut Self {
        self.collateral_type = Some(ct);
        self
    }

    /// Build the call data for AddCollateral
    pub fn build(&self) -> Result<OpenPositionCallData, StablecoinClientError> {
        let owner_secret = self.owner_secret.ok_or_else(|| StablecoinClientError::MissingField("owner_secret"))?;
        let added_collateral = self.added_collateral.ok_or_else(|| StablecoinClientError::MissingField("added_collateral"))?;
        let current_collateral = self.current_collateral.ok_or_else(|| StablecoinClientError::MissingField("current_collateral"))?;
        let current_debt = self.current_debt.ok_or_else(|| StablecoinClientError::MissingField("current_debt"))?;
        let collateral_type = self.collateral_type.ok_or_else(|| StablecoinClientError::MissingField("collateral_type"))?;

        // For add collateral, the new position is current + added
        Ok(OpenPositionCallData::new(
            owner_secret,
            current_collateral.saturating_add(added_collateral),
            current_debt,
            collateral_type,
        ))
    }
}

impl Default for AddCollateralBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for removing collateral from a position
pub struct RemoveCollateralBuilder {
    /// Position nullifier
    position_nullifier: Option<pallas::Base>,
    /// Amount of collateral to remove
    removed_collateral: Option<u64>,
    /// Owner's secret key
    owner_secret: Option<pallas::Base>,
    /// Current collateral amount
    current_collateral: Option<u64>,
    /// Current debt amount
    current_debt: Option<u64>,
}

impl RemoveCollateralBuilder {
    pub fn new() -> Self {
        Self {
            position_nullifier: None,
            removed_collateral: None,
            owner_secret: None,
            current_collateral: None,
            current_debt: None,
        }
    }

    pub fn position_nullifier(&mut self, nullifier: pallas::Base) -> &mut Self {
        self.position_nullifier = Some(nullifier);
        self
    }

    pub fn removed_collateral(&mut self, amount: u64) -> &mut Self {
        self.removed_collateral = Some(amount);
        self
    }

    pub fn owner_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.owner_secret = Some(secret);
        self
    }

    pub fn current_collateral(&mut self, amount: u64) -> &mut Self {
        self.current_collateral = Some(amount);
        self
    }

    pub fn current_debt(&mut self, amount: u64) -> &mut Self {
        self.current_debt = Some(amount);
        self
    }

    pub fn build(&self) -> Result<OpenPositionCallData, StablecoinClientError> {
        let owner_secret = self.owner_secret.ok_or_else(|| StablecoinClientError::MissingField("owner_secret"))?;
        let removed_collateral = self.removed_collateral.ok_or_else(|| StablecoinClientError::MissingField("removed_collateral"))?;
        let current_collateral = self.current_collateral.ok_or_else(|| StablecoinClientError::MissingField("current_collateral"))?;
        let current_debt = self.current_debt.ok_or_else(|| StablecoinClientError::MissingField("current_debt"))?;

        Ok(OpenPositionCallData::new(
            owner_secret,
            current_collateral.saturating_sub(removed_collateral),
            current_debt,
            pallas::Base::zero(),
        ))
    }
}

impl Default for RemoveCollateralBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for minting stablecoin against collateral
pub struct MintStableBuilder {
    /// Position nullifier
    position_nullifier: Option<pallas::Base>,
    /// Amount of stablecoin to mint
    mint_amount: Option<u64>,
    /// Owner's secret key
    owner_secret: Option<pallas::Base>,
    /// Current collateral amount
    current_collateral: Option<u64>,
    /// Current debt amount
    current_debt: Option<u64>,
}

impl MintStableBuilder {
    pub fn new() -> Self {
        Self {
            position_nullifier: None,
            mint_amount: None,
            owner_secret: None,
            current_collateral: None,
            current_debt: None,
        }
    }

    pub fn position_nullifier(&mut self, nullifier: pallas::Base) -> &mut Self {
        self.position_nullifier = Some(nullifier);
        self
    }

    pub fn mint_amount(&mut self, amount: u64) -> &mut Self {
        self.mint_amount = Some(amount);
        self
    }

    pub fn owner_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.owner_secret = Some(secret);
        self
    }

    pub fn current_collateral(&mut self, amount: u64) -> &mut Self {
        self.current_collateral = Some(amount);
        self
    }

    pub fn current_debt(&mut self, amount: u64) -> &mut Self {
        self.current_debt = Some(amount);
        self
    }

    pub fn build(&self) -> Result<MintStableCallData, StablecoinClientError> {
        let owner_secret = self.owner_secret.ok_or_else(|| StablecoinClientError::MissingField("owner_secret"))?;
        let mint_amount = self.mint_amount.ok_or_else(|| StablecoinClientError::MissingField("mint_amount"))?;
        let current_collateral = self.current_collateral.ok_or_else(|| StablecoinClientError::MissingField("current_collateral"))?;
        let current_debt = self.current_debt.ok_or_else(|| StablecoinClientError::MissingField("current_debt"))?;

        Ok(MintStableCallData::new(
            owner_secret,
            current_collateral,
            current_debt,
            mint_amount,
            BaseBlind::random(&mut OsRng),
            BaseBlind::random(&mut OsRng),
            pallas::Base::zero(), // old_commitment placeholder
        ))
    }
}

impl Default for MintStableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for repaying stablecoin debt
pub struct RepayStableBuilder {
    /// Position nullifier
    position_nullifier: Option<pallas::Base>,
    /// Amount of stablecoin to repay
    repay_amount: Option<u64>,
    /// Owner's secret key
    owner_secret: Option<pallas::Base>,
    /// Current debt amount
    current_debt: Option<u64>,
}

impl RepayStableBuilder {
    pub fn new() -> Self {
        Self {
            position_nullifier: None,
            repay_amount: None,
            owner_secret: None,
            current_debt: None,
        }
    }

    pub fn position_nullifier(&mut self, nullifier: pallas::Base) -> &mut Self {
        self.position_nullifier = Some(nullifier);
        self
    }

    pub fn repay_amount(&mut self, amount: u64) -> &mut Self {
        self.repay_amount = Some(amount);
        self
    }

    pub fn owner_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.owner_secret = Some(secret);
        self
    }

    pub fn current_debt(&mut self, amount: u64) -> &mut Self {
        self.current_debt = Some(amount);
        self
    }

    pub fn build(&self) -> Result<MintStableCallData, StablecoinClientError> {
        let owner_secret = self.owner_secret.ok_or_else(|| StablecoinClientError::MissingField("owner_secret"))?;
        let repay_amount = self.repay_amount.ok_or_else(|| StablecoinClientError::MissingField("repay_amount"))?;
        let current_debt = self.current_debt.ok_or_else(|| StablecoinClientError::MissingField("current_debt"))?;

        Ok(MintStableCallData::new(
            owner_secret,
            0, // collateral unchanged for repay
            current_debt.saturating_sub(repay_amount),
            0, // mint_amount = 0 for repay
            BaseBlind::random(&mut OsRng),
            BaseBlind::random(&mut OsRng),
            pallas::Base::zero(),
        ))
    }
}

impl Default for RepayStableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for liquidating a position
pub struct LiquidateBuilder {
    /// Owner's secret key
    owner_secret: Option<pallas::Base>,
    /// Current collateral amount
    collateral_amount: Option<u64>,
    /// Current debt amount
    debt_amount: Option<u64>,
    /// Liquidation penalty (basis points)
    liquidation_penalty: Option<u64>,
    /// Current price
    current_price: Option<u64>,
    /// Liquidator reward
    liquidator_reward: Option<u64>,
}

impl LiquidateBuilder {
    pub fn new() -> Self {
        Self {
            owner_secret: None,
            collateral_amount: None,
            debt_amount: None,
            liquidation_penalty: None,
            current_price: None,
            liquidator_reward: None,
        }
    }

    pub fn owner_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.owner_secret = Some(secret);
        self
    }

    pub fn collateral_amount(&mut self, amount: u64) -> &mut Self {
        self.collateral_amount = Some(amount);
        self
    }

    pub fn debt_amount(&mut self, amount: u64) -> &mut Self {
        self.debt_amount = Some(amount);
        self
    }

    pub fn liquidation_penalty(&mut self, penalty: u64) -> &mut Self {
        self.liquidation_penalty = Some(penalty);
        self
    }

    pub fn current_price(&mut self, price: u64) -> &mut Self {
        self.current_price = Some(price);
        self
    }

    pub fn liquidator_reward(&mut self, reward: u64) -> &mut Self {
        self.liquidator_reward = Some(reward);
        self
    }

    pub fn build(&self) -> Result<LiquidateCallData, StablecoinClientError> {
        let owner_secret = self.owner_secret.ok_or_else(|| StablecoinClientError::MissingField("owner_secret"))?;
        let collateral_amount = self.collateral_amount.ok_or_else(|| StablecoinClientError::MissingField("collateral_amount"))?;
        let debt_amount = self.debt_amount.ok_or_else(|| StablecoinClientError::MissingField("debt_amount"))?;
        let liquidation_penalty = self.liquidation_penalty.unwrap_or(1000); // default 10%
        let current_price = self.current_price.unwrap_or(0);
        let liquidator_reward = self.liquidator_reward.unwrap_or(0);

        Ok(LiquidateCallData::new(
            owner_secret,
            collateral_amount,
            debt_amount,
            liquidation_penalty,
            current_price,
            liquidator_reward,
            BaseBlind::random(&mut OsRng),
            BaseBlind::random(&mut OsRng),
            pallas::Base::zero(), // old_commitment placeholder
        ))
    }
}

impl Default for LiquidateBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Client errors for stablecoin contract operations
#[derive(Debug, thiserror::Error)]
pub enum StablecoinClientError {
    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    #[error("Invalid value: {0}")]
    InvalidValue(String),

    #[error("ZK proof error: {0}")]
    ProofError(String),
}

// ============================================================================
// Imports
// ============================================================================

use dwow_sdk::{
    crypto::{BaseBlind},
    pasta::pallas,
};
use rand::rngs::OsRng;

// Re-export call data types for convenience
pub use initialize_v1::{create_initialize_proof, InitV1CallData, InitV1PublicInputs};
pub use open_position_v1::{create_open_position_proof, OpenPositionCallData};
pub use mint_stable_v1::{create_mint_stable_proof, MintStableCallData};
pub use liquidate_v1::{create_liquidate_proof, LiquidateCallData};