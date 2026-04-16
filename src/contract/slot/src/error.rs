/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Slot Contract Errors

use darkfi_sdk::error::ContractError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SlotError {
    #[error("Spin not found")]
    SpinNotFound,

    #[error("Spin already exists")]
    SpinAlreadyExists,

    #[error("Spin not in expected state")]
    InvalidSpinState,

    #[error("Invalid reel count")]
    InvalidReelCount,

    #[error("Invalid symbol")]
    InvalidSymbol,

    #[error("Invalid payline")]
    InvalidPayline,

    #[error("Invalid bet value")]
    InvalidBetValue,

    #[error("Bet value exceeds maximum")]
    BetValueExceedsMax,

    #[error("Bet value below minimum")]
    BetValueBelowMin,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Signature verification failed")]
    InvalidSignature,

    #[error("Invalid entropy source")]
    InvalidEntropy,

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Invalid function call")]
    InvalidFunction,

    #[error("Cross-contract call failed")]
    CrossContractFailed,

    #[error("Paytable mismatch")]
    PaytableMismatch,

    #[error("House not initialized")]
    HouseNotInitialized,

    #[error("Invalid children indexes: expected money_v3::transfer_v1 calls")]
    InvalidChildrenIndexes,

    #[error("Invalid child call: expected money_v3::transfer_v1")]
    InvalidChildCall,
}

impl From<SlotError> for ContractError {
    fn from(e: SlotError) -> Self {
        match e {
            SlotError::SpinNotFound => Self::Custom(1),
            SlotError::SpinAlreadyExists => Self::Custom(2),
            SlotError::InvalidSpinState => Self::Custom(3),
            SlotError::InvalidReelCount => Self::Custom(4),
            SlotError::InvalidSymbol => Self::Custom(5),
            SlotError::InvalidPayline => Self::Custom(6),
            SlotError::InvalidBetValue => Self::Custom(7),
            SlotError::BetValueExceedsMax => Self::Custom(8),
            SlotError::BetValueBelowMin => Self::Custom(9),
            SlotError::UnauthorizedCaller => Self::Custom(10),
            SlotError::InvalidSignature => Self::Custom(11),
            SlotError::InvalidEntropy => Self::Custom(12),
            SlotError::ArithmeticOverflow => Self::Custom(13),
            SlotError::InvalidFunction => Self::Custom(14),
            SlotError::CrossContractFailed => Self::Custom(15),
            SlotError::PaytableMismatch => Self::Custom(16),
            SlotError::HouseNotInitialized => Self::Custom(18),
            SlotError::InvalidChildrenIndexes => Self::Custom(19),
            SlotError::InvalidChildCall => Self::Custom(20),
        }
    }
}