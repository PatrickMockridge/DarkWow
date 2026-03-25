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

/// Bridge contract errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum BridgeError {
    #[error("Invalid deposit: {0}")]
    InvalidDeposit(String),

    #[error("Invalid withdrawal: {0}")]
    InvalidWithdrawal(String),

    #[error("Deposit already claimed")]
    DepositAlreadyClaimed,

    #[error("Withdrawal already processed")]
    WithdrawalAlreadyProcessed,

    #[error("Invalid merkle proof")]
    InvalidMerkleProof,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Insufficient bridge fee")]
    InsufficientBridgeFee,

    #[error("Invalid external chain state")]
    InvalidExternalChainState,

    #[error("Bridge not initialized")]
    BridgeNotInitialized,

    #[error("Insufficient confirmations")]
    InsufficientConfirmations,

    #[error("Double-deposit attempt")]
    DoubleDeposit,

    #[error("Double-spend attempt")]
    DoubleSpend,

    #[error("Invalid commitment")]
    InvalidCommitment,

    #[error("Invalid nullifier")]
    InvalidNullifier,

    #[error("Commitment not found in tree")]
    CommitmentNotFound,

    #[error("Invalid ZK proof")]
    InvalidZkProof,

    #[error("Unauthorized configuration update")]
    UnauthorizedConfigUpdate,
}

impl From<BridgeError> for ContractError {
    fn from(e: BridgeError) -> Self {
        match e {
            BridgeError::InvalidDeposit(_) => Self::Custom(1),
            BridgeError::InvalidWithdrawal(_) => Self::Custom(2),
            BridgeError::DepositAlreadyClaimed => Self::Custom(3),
            BridgeError::WithdrawalAlreadyProcessed => Self::Custom(4),
            BridgeError::InvalidMerkleProof => Self::Custom(5),
            BridgeError::InvalidSignature => Self::Custom(6),
            BridgeError::InsufficientBridgeFee => Self::Custom(7),
            BridgeError::InvalidExternalChainState => Self::Custom(8),
            BridgeError::BridgeNotInitialized => Self::Custom(9),
            BridgeError::InsufficientConfirmations => Self::Custom(10),
            BridgeError::DoubleDeposit => Self::Custom(11),
            BridgeError::DoubleSpend => Self::Custom(12),
            BridgeError::InvalidCommitment => Self::Custom(13),
            BridgeError::InvalidNullifier => Self::Custom(14),
            BridgeError::CommitmentNotFound => Self::Custom(15),
            BridgeError::InvalidZkProof => Self::Custom(16),
            BridgeError::UnauthorizedConfigUpdate => Self::Custom(17),
        }
    }
}