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

//! Plain Subscription Contract Errors

use darkfi_sdk::error::ContractError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SubscriptionPlainError {
    #[error("Subscription not found")]
    SubscriptionNotFound,

    #[error("Subscription already exists")]
    SubscriptionAlreadyExists,

    #[error("Subscription expired")]
    SubscriptionExpired,

    #[error("Subscription not active")]
    SubscriptionNotActive,

    #[error("Insufficient permissions")]
    InsufficientPermissions,

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Invalid access bitmask")]
    InvalidAccessMask,

    #[error("Invalid duration")]
    InvalidDuration,

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Division by zero")]
    DivisionByZero,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Signature verification failed")]
    InvalidSignature,

    #[error("Invalid function call")]
    InvalidFunction,

    #[error("Cross-contract call failed")]
    CrossContractFailed,
}

impl From<SubscriptionPlainError> for ContractError {
    fn from(e: SubscriptionPlainError) -> Self {
        match e {
            SubscriptionPlainError::SubscriptionNotFound => Self::Custom(1),
            SubscriptionPlainError::SubscriptionAlreadyExists => Self::Custom(2),
            SubscriptionPlainError::SubscriptionExpired => Self::Custom(3),
            SubscriptionPlainError::SubscriptionNotActive => Self::Custom(4),
            SubscriptionPlainError::InsufficientPermissions => Self::Custom(5),
            SubscriptionPlainError::RateLimitExceeded => Self::Custom(6),
            SubscriptionPlainError::InvalidAccessMask => Self::Custom(7),
            SubscriptionPlainError::InvalidDuration => Self::Custom(8),
            SubscriptionPlainError::ArithmeticOverflow => Self::Custom(9),
            SubscriptionPlainError::DivisionByZero => Self::Custom(10),
            SubscriptionPlainError::UnauthorizedCaller => Self::Custom(11),
            SubscriptionPlainError::InvalidSignature => Self::Custom(12),
            SubscriptionPlainError::InvalidFunction => Self::Custom(13),
            SubscriptionPlainError::CrossContractFailed => Self::Custom(14),
        }
    }
}
