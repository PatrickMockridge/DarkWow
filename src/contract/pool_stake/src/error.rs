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

//! Pool Stake Contract Errors

use thiserror::Error;

/// Pool Stake Contract Errors
#[derive(Error, Debug)]
pub enum PoolStakeError {
    #[error("Pool not found")]
    PoolNotFound,

    #[error("Member stake not found")]
    StakeNotFound,

    #[error("Coverage allocation not found")]
    AllocationNotFound,

    #[error("Insufficient stake amount: minimum {0}")]
    InsufficientStake(u64),

    #[error("Insufficient coverage available")]
    InsufficientCoverage,

    #[error("Coverage already allocated for this withdrawal")]
    CoverageAlreadyAllocated,

    #[error("Stake is locked (cooldown period)")]
    StakeLocked,

    #[error("Not a pool member")]
    NotMember,

    #[error("Already a pool member")]
    AlreadyMember,

    #[error("Invalid coverage ratio")]
    InvalidCoverageRatio,

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Unauthorized access")]
    Unauthorized,

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Pool is full (max members reached)")]
    PoolFull,

    #[error("Cooldown not elapsed since leave request")]
    CooldownNotElapsed,

    #[error("No earnings to claim")]
    NoEarnings,

    #[error("Update failed: {0}")]
    UpdateFailed(String),

    #[error("Invalid children: expected 1 money_v3::transfer_v1 child call")]
    InvalidChildrenIndexes,

    #[error("Child call is not money_v3::transfer_v1 (0x04)")]
    InvalidChildCall,
}

impl From<PoolStakeError> for darkfi_sdk::error::ContractError {
    fn from(e: PoolStakeError) -> Self {
        match e {
            PoolStakeError::PoolNotFound => Self::Custom(1),
            PoolStakeError::StakeNotFound => Self::Custom(2),
            PoolStakeError::AllocationNotFound => Self::Custom(3),
            PoolStakeError::InsufficientStake(_) => Self::Custom(4),
            PoolStakeError::InsufficientCoverage => Self::Custom(5),
            PoolStakeError::CoverageAlreadyAllocated => Self::Custom(6),
            PoolStakeError::StakeLocked => Self::Custom(7),
            PoolStakeError::NotMember => Self::Custom(8),
            PoolStakeError::AlreadyMember => Self::Custom(9),
            PoolStakeError::InvalidCoverageRatio => Self::Custom(10),
            PoolStakeError::ArithmeticOverflow => Self::Custom(11),
            PoolStakeError::Unauthorized => Self::Custom(12),
            PoolStakeError::InvalidParams(_) => Self::Custom(13),
            PoolStakeError::PoolFull => Self::Custom(14),
            PoolStakeError::CooldownNotElapsed => Self::Custom(15),
            PoolStakeError::NoEarnings => Self::Custom(16),
            PoolStakeError::UpdateFailed(_) => Self::Custom(17),
            PoolStakeError::InvalidChildrenIndexes => Self::Custom(18),
            PoolStakeError::InvalidChildCall => Self::Custom(19),
        }
    }
}