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

//! Plain Labor Market Contract Errors

use darkfi_sdk::error::ContractError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LaborMarketPlainError {
    #[error("Job not found")]
    JobNotFound,

    #[error("Job already exists")]
    JobAlreadyExists,

    #[error("Job not in expected state")]
    InvalidJobState,

    #[error("Job not active")]
    JobNotActive,

    #[error("Invalid deadline")]
    InvalidDeadline,

    #[error("Deadline already passed")]
    DeadlinePassed,

    #[error("Insufficient payment")]
    InsufficientPayment,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Signature verification failed")]
    InvalidSignature,

    #[error("Milestone not found")]
    MilestoneNotFound,

    #[error("Milestone already completed")]
    MilestoneAlreadyCompleted,

    #[error("Invalid deliverable")]
    InvalidDeliverable,

    #[error("Payment transfer failed")]
    PaymentFailed,

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Division by zero")]
    DivisionByZero,

    #[error("Invalid function call")]
    InvalidFunction,

    #[error("Cross-contract call failed")]
    CrossContractFailed,
}

impl From<LaborMarketPlainError> for ContractError {
    fn from(e: LaborMarketPlainError) -> Self {
        match e {
            LaborMarketPlainError::JobNotFound => Self::Custom(1),
            LaborMarketPlainError::JobAlreadyExists => Self::Custom(2),
            LaborMarketPlainError::InvalidJobState => Self::Custom(3),
            LaborMarketPlainError::JobNotActive => Self::Custom(4),
            LaborMarketPlainError::InvalidDeadline => Self::Custom(5),
            LaborMarketPlainError::DeadlinePassed => Self::Custom(6),
            LaborMarketPlainError::InsufficientPayment => Self::Custom(7),
            LaborMarketPlainError::UnauthorizedCaller => Self::Custom(8),
            LaborMarketPlainError::InvalidSignature => Self::Custom(9),
            LaborMarketPlainError::MilestoneNotFound => Self::Custom(10),
            LaborMarketPlainError::MilestoneAlreadyCompleted => Self::Custom(11),
            LaborMarketPlainError::InvalidDeliverable => Self::Custom(12),
            LaborMarketPlainError::PaymentFailed => Self::Custom(13),
            LaborMarketPlainError::ArithmeticOverflow => Self::Custom(14),
            LaborMarketPlainError::DivisionByZero => Self::Custom(15),
            LaborMarketPlainError::InvalidFunction => Self::Custom(16),
            LaborMarketPlainError::CrossContractFailed => Self::Custom(17),
        }
    }
}