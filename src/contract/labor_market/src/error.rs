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

//! Labor Market Contract Errors

use darkfi_sdk::error::ContractError;
use thiserror::Error;

/// Contract error types
#[derive(Debug, Error)]
pub enum LaborMarketError {
    #[error("Job not found")]
    JobNotFound,

    #[error("Invalid job state transition")]
    InvalidStateTransition,

    #[error("Job already exists")]
    JobAlreadyExists,

    #[error("Deadline has passed")]
    DeadlinePassed,

    #[error("Deadline not yet passed")]
    DeadlineNotPassed,

    #[error("Deliverable hash mismatch")]
    DeliverableHashMismatch,

    #[error("Worker already assigned to this job")]
    WorkerAlreadyAssigned,

    #[error("No worker assigned to this job")]
    NoWorkerAssigned,

    #[error("Not authorized: must be employer or worker")]
    NotAuthorized,

    #[error("Not authorized: must be employer")]
    NotEmployer,

    #[error("Not authorized: must be worker")]
    NotWorker,

    #[error("Already submitted")]
    AlreadySubmitted,

    #[error("Already refunded or claimed")]
    AlreadySpent,

    #[error("Job not in correct state")]
    IncorrectJobState,

    #[error("ZK proof verification failed")]
    ZkProofVerificationFailed,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Invalid delivery type")]
    InvalidDeliveryType,

    #[error("Invalid attestation claim")]
    InvalidClaim,

    #[error("DAO-Escrow required for dispute")]
    DaoEscrowRequired,

    #[error("Sled database error: {0}")]
    SledError(String),

    // Milestone-specific errors
    #[error("Invalid milestone index")]
    InvalidMilestoneIndex,

    #[error("Milestone already completed")]
    MilestoneAlreadyCompleted,

    #[error("Milestone deadline not reached")]
    MilestoneDeadlineNotReached,

    #[error("Invalid milestone payment amount")]
    InvalidMilestonePaymentAmount,

    #[error("Job does not have milestones")]
    JobDoesNotHaveMilestones,

    #[error("Milestone out of order")]
    MilestoneOutOfOrder,

    // O-Cap capability errors
    #[error("Job requires capability")]
    CapabilityRequired,

    #[error("Capability requirement not met")]
    CapabilityNotMet,

    #[error("Invalid capability")]
    InvalidCapability,

    #[error("Capability revoked")]
    CapabilityRevoked,

    #[error("Invalid children indexes")]
    InvalidChildrenIndexes,

    #[error("Invalid child call")]
    InvalidChildCall,
}

impl From<LaborMarketError> for ContractError {
    fn from(e: LaborMarketError) -> Self {
        match e {
            LaborMarketError::JobNotFound => Self::Custom(1),
            LaborMarketError::InvalidStateTransition => Self::Custom(2),
            LaborMarketError::JobAlreadyExists => Self::Custom(3),
            LaborMarketError::DeadlinePassed => Self::Custom(4),
            LaborMarketError::DeadlineNotPassed => Self::Custom(5),
            LaborMarketError::DeliverableHashMismatch => Self::Custom(6),
            LaborMarketError::WorkerAlreadyAssigned => Self::Custom(7),
            LaborMarketError::NoWorkerAssigned => Self::Custom(8),
            LaborMarketError::NotAuthorized => Self::Custom(9),
            LaborMarketError::NotEmployer => Self::Custom(10),
            LaborMarketError::NotWorker => Self::Custom(11),
            LaborMarketError::AlreadySubmitted => Self::Custom(12),
            LaborMarketError::AlreadySpent => Self::Custom(13),
            LaborMarketError::IncorrectJobState => Self::Custom(14),
            LaborMarketError::ZkProofVerificationFailed => Self::Custom(15),
            LaborMarketError::InvalidSignature => Self::Custom(16),
            LaborMarketError::InvalidDeliveryType => Self::Custom(17),
            LaborMarketError::InvalidClaim => Self::Custom(18),
            LaborMarketError::DaoEscrowRequired => Self::Custom(19),
            LaborMarketError::SledError(_) => Self::Custom(20),
            LaborMarketError::InvalidMilestoneIndex => Self::Custom(21),
            LaborMarketError::MilestoneAlreadyCompleted => Self::Custom(22),
            LaborMarketError::MilestoneDeadlineNotReached => Self::Custom(23),
            LaborMarketError::InvalidMilestonePaymentAmount => Self::Custom(24),
            LaborMarketError::JobDoesNotHaveMilestones => Self::Custom(25),
            LaborMarketError::MilestoneOutOfOrder => Self::Custom(26),
            LaborMarketError::CapabilityRequired => Self::Custom(27),
            LaborMarketError::CapabilityNotMet => Self::Custom(28),
            LaborMarketError::InvalidCapability => Self::Custom(29),
            LaborMarketError::CapabilityRevoked => Self::Custom(30),
            LaborMarketError::InvalidChildrenIndexes => Self::Custom(31),
            LaborMarketError::InvalidChildCall => Self::Custom(32),
        }
    }
}
