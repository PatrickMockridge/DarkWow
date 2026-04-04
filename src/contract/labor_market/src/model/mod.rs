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

//! Labor Market Contract Data Structures

use serde::{Deserialize, Serialize};

/// Delivery type for job work
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DeliveryType {
    /// Generic deliverable: hash of a zip file
    Generic = 0,
    /// Git deliverable: commit hash
    Git = 1,
}

impl Default for DeliveryType {
    fn default() -> Self {
        Self::Generic
    }
}

// ============================================================================
// MILESTONE (For multi-stage jobs with time-weighted payments)
// ============================================================================

/// A milestone in a multi-stage job
/// Each milestone has its own deadline and payment amount
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    /// Milestone index (0-based)
    pub index: u32,
    /// Payment amount for this milestone
    pub payment_amount: u64,
    /// Deadline block for this milestone
    pub deadline_block: u64,
    /// Whether this milestone has been completed
    pub completed: bool,
    /// Block when completed
    pub completed_at_block: Option<u64>,
}

impl Default for Milestone {
    fn default() -> Self {
        Self { index: 0, payment_amount: 0, deadline_block: 0, completed: false, completed_at_block: None }
    }
}

/// Job state in the state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum JobState {
    /// Job created, awaiting worker acceptance
    Created = 0,
    /// Worker accepted, working on deliverable
    InProgress = 1,
    /// Work delivered, awaiting confirmation
    Delivered = 2,
    /// Employer confirmed, payment released
    Confirmed = 3,
    /// Escalated to DAO for dispute resolution
    Disputed = 4,
    /// Timeout, employer refunded
    Refunded = 5,
    /// Cancelled before acceptance
    Cancelled = 6,
}

impl Default for JobState {
    fn default() -> Self {
        Self::Created
    }
}

/// A job posting in the labor market
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique job identifier (Poseidon hash commitment)
    pub id: pallas::Base,
    /// Employer's public key
    pub employer_pubkey: [pallas::Base; 2],
    /// Worker's public key (set when accepted)
    pub worker_pubkey: Option<[pallas::Base; 2]>,
    /// Attestation ID for deliverable verification (references attestation contract)
    pub attestation_id: pallas::Base,
    /// Type of deliverable (generic or git)
    pub delivery_type: DeliveryType,
    /// Payment amount
    pub payment_amount: u64,
    /// Token being paid
    pub payment_token: pallas::Base,
    /// Payment commitment (Pedersen)
    pub payment_commit: [pallas::Base; 2],
    /// Block by which work must be delivered
    pub deadline_block: u64,
    /// Current job state
    pub state: JobState,
    /// DAO-Escrow bulla for dispute resolution
    pub dao_escrow_bulla: Option<pallas::Base>,
    /// Milestones for this job (empty if no milestones)
    pub milestones: Vec<Milestone>,
    /// Current milestone index (0-based)
    pub current_milestone: u32,
    /// Accumulated payment released so far
    pub released_payment: u64,
}

/// Parameters for creating a new job
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateJobParamsV1 {
    /// ZK proof for job creation
    pub proof: Vec<u8>,
    /// Job ID (public input)
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Attestation ID for deliverable verification (references attestation contract)
    pub attestation_id: pallas::Base,
    /// Type of deliverable (0 = Generic, 1 = Git)
    pub delivery_type: u8,
    /// Payment amount
    pub payment_amount: u64,
    /// Token being paid
    pub payment_token: pallas::Base,
    /// Payment commitment x coordinate
    pub payment_commit_x: pallas::Base,
    /// Payment commitment y coordinate
    pub payment_commit_y: pallas::Base,
}

/// Parameters for accepting a job
#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptJobParamsV1 {
    /// ZK proof for job acceptance
    pub proof: Vec<u8>,
    /// Job ID being accepted
    pub job_id: pallas::Base,
    /// Worker's public key x coordinate
    pub worker_pub_x: pallas::Base,
    /// Worker's public key y coordinate
    pub worker_pub_y: pallas::Base,
}

/// Parameters for submitting a generic deliverable (zip hash)
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitDeliverableParamsV1 {
    /// ZK proof for deliverable submission
    pub proof: Vec<u8>,
    /// Job ID being completed
    pub job_id: pallas::Base,
    /// Attestation claim ID (from attestation.create_claim)
    pub claim_id: pallas::Base,
    /// Worker's public key x coordinate
    pub worker_pub_x: pallas::Base,
    /// Worker's public key y coordinate
    pub worker_pub_y: pallas::Base,
    /// Nullifier for preventing double-submission
    pub spent_nullifier: pallas::Base,
}

/// Parameters for submitting a git deliverable (commit hash)
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitGitDeliverableParamsV1 {
    /// ZK proof for git deliverable submission
    pub proof: Vec<u8>,
    /// Job ID being completed
    pub job_id: pallas::Base,
    /// Attestation claim ID (from attestation.create_claim)
    pub claim_id: pallas::Base,
    /// Worker's public key x coordinate
    pub worker_pub_x: pallas::Base,
    /// Worker's public key y coordinate
    pub worker_pub_y: pallas::Base,
    /// Nullifier for preventing double-submission
    pub spent_nullifier: pallas::Base,
}

/// Parameters for confirming delivery and releasing payment
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfirmDeliveryParamsV1 {
    /// ZK proof for confirmation
    pub proof: Vec<u8>,
    /// Job ID being confirmed
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Nullifier for release authorization
    pub spent_nullifier: pallas::Base,
}

/// Parameters for escalating to DAO dispute resolution
#[derive(Debug, Serialize, Deserialize)]
pub struct DisputeParamsV1 {
    /// ZK proof for dispute
    pub proof: Vec<u8>,
    /// Job ID being disputed
    pub job_id: pallas::Base,
    /// Disputer's public key x coordinate
    pub disputer_pub_x: pallas::Base,
    /// Disputer's public key y coordinate
    pub disputer_pub_y: pallas::Base,
    /// DAO-Escrow handling the dispute
    pub dao_escrow_bulla: pallas::Base,
    /// Nullifier for dispute
    pub spent_nullifier: pallas::Base,
}

/// Parameters for timeout refund
#[derive(Debug, Serialize, Deserialize)]
pub struct RefundParamsV1 {
    /// ZK proof for refund
    pub proof: Vec<u8>,
    /// Job ID being refunded
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Nullifier for refund authorization
    pub spent_nullifier: pallas::Base,
}

/// Parameters for cancelling a job before acceptance
#[derive(Debug, Serialize, Deserialize)]
pub struct CancelJobParamsV1 {
    /// ZK proof for cancellation
    pub proof: Vec<u8>,
    /// Job ID being cancelled
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
}

// ============================================================================
// MILESTONE-ENHANCED PARAMETERS (For jobs with milestones)
// ============================================================================

/// Parameters for creating a job with milestones
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateJobWithMilestonesParamsV1 {
    /// ZK proof for job creation
    pub proof: Vec<u8>,
    /// Job ID (public input)
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Attestation ID for deliverable verification
    pub attestation_id: pallas::Base,
    /// Type of deliverable (0 = Generic, 1 = Git)
    pub delivery_type: u8,
    /// Total payment amount (sum of all milestones)
    pub payment_amount: u64,
    /// Token being paid
    pub payment_token: pallas::Base,
    /// Payment commitment x coordinate
    pub payment_commit_x: pallas::Base,
    /// Payment commitment y coordinate
    pub payment_commit_y: pallas::Base,
    /// Overall deadline block
    pub deadline_block: u64,
    /// Number of milestones
    pub milestone_count: u32,
}

/// Parameters for submitting a deliverable for a specific milestone
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitMilestoneDeliverableParamsV1 {
    /// ZK proof for milestone deliverable submission
    pub proof: Vec<u8>,
    /// Job ID being completed
    pub job_id: pallas::Base,
    /// Milestone index being submitted
    pub milestone_index: u32,
    /// Attestation claim ID
    pub claim_id: pallas::Base,
    /// Worker's public key x coordinate
    pub worker_pub_x: pallas::Base,
    /// Worker's public key y coordinate
    pub worker_pub_y: pallas::Base,
    /// Nullifier for preventing double-submission
    pub spent_nullifier: pallas::Base,
}

/// Parameters for confirming a milestone and releasing payment
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfirmMilestoneParamsV1 {
    /// ZK proof for milestone confirmation
    pub proof: Vec<u8>,
    /// Job ID being confirmed
    pub job_id: pallas::Base,
    /// Milestone index being confirmed
    pub milestone_index: u32,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Payment release amount for this milestone
    pub payment_release: u64,
    /// Nullifier for release authorization
    pub spent_nullifier: pallas::Base,
}

/// Parameters for raising a dispute for a specific milestone
#[derive(Debug, Serialize, Deserialize)]
pub struct InitiateDisputeParamsV1 {
    /// ZK proof for dispute
    pub proof: Vec<u8>,
    /// Job ID being disputed
    pub job_id: pallas::Base,
    /// Milestone index being disputed
    pub milestone_index: u32,
    /// Disputer's public key x coordinate
    pub disputer_pub_x: pallas::Base,
    /// Disputer's public key y coordinate
    pub disputer_pub_y: pallas::Base,
    /// DAO-Escrow handling the dispute
    pub dao_escrow_bulla: pallas::Base,
    /// Nullifier for dispute
    pub spent_nullifier: pallas::Base,
}
