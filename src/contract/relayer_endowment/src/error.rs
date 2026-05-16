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

//! Relayer Endowment Contract Errors

use thiserror::Error;

/// Relayer Endowment Contract Errors
#[derive(Error, Debug)]
pub enum RelayerEndowmentError {
    #[error("Endowment account not found")]
    EndowmentNotFound,

    #[error("Deployment not found")]
    DeploymentNotFound,

    #[error("Insufficient deployment amount: minimum {0}")]
    InsufficientDeploy(u64),

    #[error("Deployment already withdrawn")]
    DeploymentAlreadyWithdrawn,

    #[error("No fees to claim")]
    NoFees,

    #[error("Unauthorized access")]
    Unauthorized,

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Update failed: {0}")]
    UpdateFailed(String),

    #[error("Invalid children indexes for child call")]
    InvalidChildrenIndexes,

    #[error("Invalid child call")]
    InvalidChildCall,

    #[error("Endowment account is inactive")]
    EndpointInactive,

    #[error("Settlement not yet due — timeout not elapsed")]
    SettlementNotDue,
}

impl From<RelayerEndowmentError> for dwow_sdk::error::ContractError {
    fn from(e: RelayerEndowmentError) -> Self {
        match e {
            RelayerEndowmentError::EndowmentNotFound => Self::Custom(1),
            RelayerEndowmentError::DeploymentNotFound => Self::Custom(2),
            RelayerEndowmentError::InsufficientDeploy(_) => Self::Custom(3),
            RelayerEndowmentError::DeploymentAlreadyWithdrawn => Self::Custom(4),
            RelayerEndowmentError::NoFees => Self::Custom(5),
            RelayerEndowmentError::Unauthorized => Self::Custom(6),
            RelayerEndowmentError::InvalidParams(_) => Self::Custom(7),
            RelayerEndowmentError::ArithmeticOverflow => Self::Custom(8),
            RelayerEndowmentError::UpdateFailed(_) => Self::Custom(9),
            RelayerEndowmentError::InvalidChildrenIndexes => Self::Custom(10),
            RelayerEndowmentError::InvalidChildCall => Self::Custom(11),
            RelayerEndowmentError::EndpointInactive => Self::Custom(12),
            RelayerEndowmentError::SettlementNotDue => Self::Custom(13),
        }
    }
}