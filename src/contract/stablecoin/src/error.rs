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

use dwow_sdk::error::ContractError;

#[derive(Debug, Clone, thiserror::Error)]
pub enum StablecoinError {
    #[error("Position commitment not found in tree")]
    PositionCommitmentNotFound,

    #[error("Position already exists")]
    PositionAlreadyExists,

    #[error("Invalid collateralization ratio")]
    InvalidCollateralizationRatio,

    #[error("Position undercollateralized")]
    Undercollateralized,

    #[error("Insufficient collateral")]
    InsufficientCollateral,

    #[error("Insufficient debt capacity")]
    InsufficientDebtCapacity,

    #[error("Repay amount exceeds debt")]
    RepayExceedsDebt,

    #[error("Remove collateral exceeds available")]
    RemoveCollateralExceedsAvailable,

    #[error("Liquidation below threshold")]
    LiquidationBelowThreshold,

    #[error("Position not liquidatable")]
    PositionNotLiquidatable,

    #[error("Invalid price feed")]
    InvalidPriceFeed,

    #[error("TWAP outside acceptable window")]
    TwapOutsideWindow,

    #[error("PI controller error")]
    PiControllerError,

    #[error("Redemption rate out of bounds")]
    RedemptionRateOutOfBounds,

    #[error("Invalid nullifier")]
    InvalidNullifier,

    #[error("Duplicate nullifier")]
    DuplicateNullifier,

    #[error("Commitment mismatch")]
    CommitmentMismatch,

    #[error("Invalid Merkle proof")]
    InvalidMerkleProof,

    #[error("Stablecoin supply overflow")]
    SupplyOverflow,

    #[error("Invalid circuit proof")]
    InvalidProof,

    #[error("Parent call function mismatch")]
    ParentCallFunctionMismatch,

    #[error("Parent call input mismatch")]
    ParentCallInputMismatch,

    #[error("Child call function mismatch")]
    ChildCallFunctionMismatch,

    #[error("Child call input mismatch")]
    ChildCallInputMismatch,

    #[error("Invalid children_indexes: expected 1 money_v3::transfer_v1 child call")]
    InvalidChildrenIndexes,

    #[error("Invalid child call: expected money_v3::transfer_v1 (0x04)")]
    InvalidChildCall,

    #[error("Invalid public input: failed to deserialize public key bytes")]
    InvalidPublicInput,
}

impl From<StablecoinError> for ContractError {
    fn from(e: StablecoinError) -> Self {
        match e {
            StablecoinError::PositionCommitmentNotFound => Self::Custom(1),
            StablecoinError::PositionAlreadyExists => Self::Custom(2),
            StablecoinError::InvalidCollateralizationRatio => Self::Custom(3),
            StablecoinError::Undercollateralized => Self::Custom(4),
            StablecoinError::InsufficientCollateral => Self::Custom(5),
            StablecoinError::InsufficientDebtCapacity => Self::Custom(6),
            StablecoinError::RepayExceedsDebt => Self::Custom(7),
            StablecoinError::RemoveCollateralExceedsAvailable => Self::Custom(8),
            StablecoinError::LiquidationBelowThreshold => Self::Custom(9),
            StablecoinError::PositionNotLiquidatable => Self::Custom(10),
            StablecoinError::InvalidPriceFeed => Self::Custom(11),
            StablecoinError::TwapOutsideWindow => Self::Custom(12),
            StablecoinError::PiControllerError => Self::Custom(13),
            StablecoinError::RedemptionRateOutOfBounds => Self::Custom(14),
            StablecoinError::InvalidNullifier => Self::Custom(15),
            StablecoinError::DuplicateNullifier => Self::Custom(16),
            StablecoinError::CommitmentMismatch => Self::Custom(17),
            StablecoinError::InvalidMerkleProof => Self::Custom(18),
            StablecoinError::SupplyOverflow => Self::Custom(19),
            StablecoinError::InvalidProof => Self::Custom(20),
            StablecoinError::ParentCallFunctionMismatch => Self::Custom(21),
            StablecoinError::ParentCallInputMismatch => Self::Custom(22),
            StablecoinError::ChildCallFunctionMismatch => Self::Custom(23),
            StablecoinError::ChildCallInputMismatch => Self::Custom(24),
            StablecoinError::InvalidChildrenIndexes => Self::Custom(25),
            StablecoinError::InvalidChildCall => Self::Custom(26),
            StablecoinError::InvalidPublicInput => Self::Custom(27),
        }
    }
}