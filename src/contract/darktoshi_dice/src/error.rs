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

use darkfi_sdk::error::ContractError;

#[derive(Debug, Clone, thiserror::Error)]
pub enum DiceError {
    #[error("Bet does not exist")]
    BetNotFound,

    #[error("Bet already exists")]
    BetAlreadyExists,

    #[error("Invalid bet state transition")]
    InvalidStateTransition,

    #[error("Invalid function called")]
    InvalidFunction,

    #[error("Target out of range (must be 1-99)")]
    InvalidTarget,

    #[error("Bet value too small")]
    BetValueTooSmall,

    #[error("Roll already revealed")]
    RollAlreadyRevealed,

    #[error("Bet not revealed yet")]
    BetNotRevealed,

    #[error("Invalid roll result")]
    InvalidRoll,

    #[error("Roll timeout not reached")]
    RollTimeoutNotReached,

    #[error("House edge out of allowed range")]
    InvalidHouseEdge,

    #[error("Signature verification failed")]
    InvalidSignature,

    #[error("Zero-knowledge proof verification failed")]
    InvalidProof,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Arithmetic overflow in calculation")]
    ArithmeticOverflow,

    #[error("Cross-contract call failed")]
    CrossContractFailed,

    #[error("Value commitment mismatch")]
    ValueCommitmentMismatch,

    #[error("Duplicate nullifier (double spend attempt)")]
    DuplicateNullifier,

    #[error("House not initialized")]
    HouseNotInitialized,

    #[error("Invalid block hash")]
    InvalidBlockHash,

    #[error("Commitment does not match bet parameters")]
    CommitmentMismatch,
}

impl From<DiceError> for ContractError {
    fn from(e: DiceError) -> Self {
        match e {
            DiceError::BetNotFound => Self::Custom(1),
            DiceError::BetAlreadyExists => Self::Custom(2),
            DiceError::InvalidStateTransition => Self::Custom(3),
            DiceError::InvalidFunction => Self::Custom(4),
            DiceError::InvalidTarget => Self::Custom(5),
            DiceError::BetValueTooSmall => Self::Custom(6),
            DiceError::RollAlreadyRevealed => Self::Custom(7),
            DiceError::BetNotRevealed => Self::Custom(8),
            DiceError::InvalidRoll => Self::Custom(9),
            DiceError::RollTimeoutNotReached => Self::Custom(10),
            DiceError::InvalidHouseEdge => Self::Custom(11),
            DiceError::InvalidSignature => Self::Custom(12),
            DiceError::InvalidProof => Self::Custom(13),
            DiceError::UnauthorizedCaller => Self::Custom(14),
            DiceError::CrossContractFailed => Self::Custom(15),
            DiceError::ValueCommitmentMismatch => Self::Custom(16),
            DiceError::DuplicateNullifier => Self::Custom(17),
            DiceError::HouseNotInitialized => Self::Custom(18),
            DiceError::InvalidBlockHash => Self::Custom(19),
            DiceError::CommitmentMismatch => Self::Custom(20),
            DiceError::ArithmeticOverflow => Self::Custom(21),
        }
    }
}
