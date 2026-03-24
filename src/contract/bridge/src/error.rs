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

use thiserror::Error;

/// Bridge contract errors
#[derive(Error, Debug)]
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

    #[error("Invalid VSS share")]
    InvalidVssShare,

    #[error("Threshold not reached")]
    ThresholdNotReached,
}
