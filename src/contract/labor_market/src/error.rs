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
}
