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
pub enum BaccaratError {
    #[error("Bet does not exist")]
    BetNotFound,

    #[error("Bet already exists")]
    BetAlreadyExists,

    #[error("Invalid bet state transition")]
    InvalidStateTransition,

    #[error("Invalid function called")]
    InvalidFunction,

    #[error("Invalid bet type (must be 0=Player, 1=Banker, 2=Tie)")]
    InvalidBetType,

    #[error("Bet value too small")]
    BetValueTooSmall,

    #[error("Cards already drawn")]
    CardsAlreadyDrawn,

    #[error("Cards not drawn yet")]
    CardsNotDrawn,

    #[error("Invalid card value")]
    InvalidCard,

    #[error("Bet timeout not reached")]
    BetTimeoutNotReached,

    #[error("House edge out of allowed range")]
    InvalidHouseEdge,

    #[error("Signature verification failed")]
    InvalidSignature,

    #[error("Zero-knowledge proof verification failed")]
    InvalidProof,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

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

    #[error("Confirmation depth out of range")]
    InvalidConfirmationDepth,
}

impl From<BaccaratError> for ContractError {
    fn from(e: BaccaratError) -> Self {
        match e {
            BaccaratError::BetNotFound => Self::Custom(1),
            BaccaratError::BetAlreadyExists => Self::Custom(2),
            BaccaratError::InvalidStateTransition => Self::Custom(3),
            BaccaratError::InvalidFunction => Self::Custom(4),
            BaccaratError::InvalidBetType => Self::Custom(5),
            BaccaratError::BetValueTooSmall => Self::Custom(6),
            BaccaratError::CardsAlreadyDrawn => Self::Custom(7),
            BaccaratError::CardsNotDrawn => Self::Custom(8),
            BaccaratError::InvalidCard => Self::Custom(9),
            BaccaratError::BetTimeoutNotReached => Self::Custom(10),
            BaccaratError::InvalidHouseEdge => Self::Custom(11),
            BaccaratError::InvalidSignature => Self::Custom(12),
            BaccaratError::InvalidProof => Self::Custom(13),
            BaccaratError::UnauthorizedCaller => Self::Custom(14),
            BaccaratError::CrossContractFailed => Self::Custom(15),
            BaccaratError::ValueCommitmentMismatch => Self::Custom(16),
            BaccaratError::DuplicateNullifier => Self::Custom(17),
            BaccaratError::HouseNotInitialized => Self::Custom(18),
            BaccaratError::InvalidBlockHash => Self::Custom(19),
            BaccaratError::CommitmentMismatch => Self::Custom(20),
            BaccaratError::InvalidConfirmationDepth => Self::Custom(21),
        }
    }
}
