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

//! Plain Labor Market Contract Model
//!
//! # Privacy Notice
//!
//! This contract uses **partial transparency** - state is public on-chain.
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full details.
//!
//! # ZK vs Native Operations
//!
//! | Operation | Method | Reason |
//! |-----------|--------|--------|
//! | Signature verification | ZK (Schnorr) | Sound, constrainable |
//! | Payment commitment | ZK (Pedersen) | Privacy-preserving |
//! | Time-weighted release | Native Rust | Needs `base_div` (not in ZK) |
//! | Milestone progress | Native Rust | Arbitrary logic |
//! | Delivery verification | Hybrid | ZK for sound parts, plain for complex logic |

use darkfi_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use darkfi_sdk::crypto::schnorr::Signature;
use darkfi_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// JOB STATE (State machine - visible on-chain)
// ============================================================================

/// Job state in the state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum JobState {
    /// Job created, awaiting worker acceptance
    Created = 0,
    /// Worker accepted, working on deliverable
    InProgress = 1,
    /// Work delivered, awaiting confirmation
    Delivered = 2,
    /// Employer confirmed, payment released
    Confirmed = 3,
    /// Dispute raised, DAO resolution needed
    Disputed = 4,
    /// Timeout, employer refunded
    Refunded = 5,
    /// Cancelled before acceptance
    Cancelled = 6,
}

/// Delivery type for job work
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum DeliveryType {
    /// Generic deliverable: hash of a zip file or similar
    Generic = 0,
    /// Git deliverable: commit hash
    Git = 1,
}

impl DeliveryType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(DeliveryType::Generic),
            1 => Some(DeliveryType::Git),
            _ => None,
        }
    }
}

// ============================================================================
// MILESTONE (For multi-stage jobs - Native Rust, visible on-chain)
// ============================================================================

/// A milestone in a multi-stage job
/// PRIVACY NOTICE: All milestone data is PUBLIC in plain version
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Milestone {
    /// Milestone index (0-based)
    pub index: u32,
    /// Hash of description of what must be delivered
    /// PRIVACY NOTICE: Actual description is off-chain, only hash is public
    pub description_hash: pallas::Base,
    /// Payment amount for this milestone
    pub payment_amount: u64,
    /// Deadline block for this milestone
    pub deadline_block: u64,
    /// Whether this milestone has been completed
    pub completed: bool,
    /// Block when completed
    pub completed_at_block: Option<u64>,
}

impl Milestone {
    /// OPCODE PLACEHOLDER: When base_div is available in ZK, milestone
    /// completion ratios could be constrained privately.
    /// Currently uses native Rust (visible on-chain).
    pub fn calculate_partial_payment(&self, elapsed_blocks: u64, total_blocks: u64) -> u64 {
        if self.completed {
            return self.payment_amount
        }

        if total_blocks == 0 {
            return 0
        }

        // Proportional payment based on time elapsed
        // PRIVACY NOTICE: This calculation is visible on-chain
        // OPCODE PLACEHOLDER: When base_div is in ZK, this could be private
        self.payment_amount * elapsed_blocks / total_blocks
    }
}

// ============================================================================
// JOB (Plain - all fields visible except work content)
// ============================================================================

/// A job in the labor market
/// PRIVACY NOTICE: Most fields are PUBLIC in plain version.
/// Actual work content (files, code, etc.) is NOT stored on-chain.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Job {
    /// Unique job identifier (Poseidon hash)
    pub id: pallas::Base,
    /// Employer's public key
    pub employer: PublicKey,
    /// Worker's public key (set when accepted)
    pub worker: Option<PublicKey>,
    /// Hash of job title/description
    /// PRIVACY NOTICE: Actual title is off-chain, only hash is public
    pub title_hash: pallas::Base,
    /// Hash of detailed specification
    /// PRIVACY NOTICE: Actual specification is off-chain, only hash is public
    pub specification_hash: pallas::Base,
    /// Delivery type (generic or git)
    pub delivery_type: DeliveryType,
    /// Total payment amount (sum of milestones)
    /// PRIVACY NOTICE: This is PUBLIC in plain version
    pub total_payment: u64,
    /// Token for payment
    pub payment_token: pallas::Base,
    /// Current job state
    pub state: JobState,
    /// Milestones for this job
    /// PRIVACY NOTICE: All milestone data is PUBLIC
    pub milestones: Vec<Milestone>,
    /// Block when job was created
    pub created_at_block: u64,
    /// Overall deadline block
    pub deadline_block: u64,
    /// Accumulated payment released so far
    pub released_payment: u64,
    /// Signature from employer for authorization
    /// ZK: Schnorr signature verified in ZK
    pub employer_signature: Option<Signature>,
}

// ============================================================================
// PARAMETERS (Input types for contract calls)
// ============================================================================

/// Parameters for creating a new job
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateJobParamsV1 {
    /// Employer's public key
    pub employer: PublicKey,
    /// Hash of job title
    /// Actual title is off-chain, only hash is stored
    pub title_hash: pallas::Base,
    /// Hash of detailed specification
    /// Actual specification is off-chain, only hash is stored
    pub specification_hash: pallas::Base,
    /// Delivery type
    pub delivery_type: DeliveryType,
    /// Total payment amount
    pub total_payment: u64,
    /// Token for payment
    pub payment_token: pallas::Base,
    /// Overall deadline block
    pub deadline_block: u64,
    /// Milestones for this job
    pub milestones: Vec<Milestone>,
    /// Employer's signature over job params
    /// ZK: Schnorr signature verified in ZK to constrain employer
    pub signature: Signature,
}

/// Parameters for accepting a job
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AcceptJobParamsV1 {
    /// Job ID
    pub job_id: pallas::Base,
    /// Worker's public key
    pub worker: PublicKey,
    /// Worker's signature over job ID
    /// ZK: Schnorr signature verified in ZK to constrain worker
    pub signature: Signature,
}

/// Parameters for submitting a deliverable
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SubmitDeliverableParamsV1 {
    /// Job ID
    pub job_id: pallas::Base,
    /// Milestone index (0-based)
    pub milestone_index: u32,
    /// Hash of the delivered work
    /// PRIVACY NOTICE: This hash is PUBLIC on-chain
    /// Actual content is off-chain
    pub deliverable_hash: pallas::Base,
    /// Worker's signature over deliverable
    /// ZK: Schnorr signature verified in ZK
    pub signature: Signature,
}

/// Parameters for confirming a deliverable
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConfirmDeliverableParamsV1 {
    /// Job ID
    pub job_id: pallas::Base,
    /// Milestone index
    pub milestone_index: u32,
    /// Employer's signature over confirmation
    /// ZK: Schnorr signature verified in ZK
    pub signature: Signature,
}

/// Parameters for raising a dispute
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DisputeParamsV1 {
    /// Job ID
    pub job_id: pallas::Base,
    /// Reason for dispute (hashed for privacy)
    /// PRIVACY NOTICE: Dispute reason is hashed but hash is visible
    pub dispute_reason_hash: pallas::Base,
    /// Party raising dispute (employer or worker)
    pub disputor: PublicKey,
    /// Signature over dispute
    pub signature: Signature,
}

/// Parameters for cancelling a job
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelJobParamsV1 {
    /// Job ID
    pub job_id: pallas::Base,
    /// Employer's signature over cancellation
    pub signature: Signature,
}

/// Parameters for requesting refund after timeout
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RefundParamsV1 {
    /// Job ID
    pub job_id: pallas::Base,
    /// Caller (should be employer)
    pub caller: PublicKey,
    /// Signature over refund request
    pub signature: Signature,
}

// ============================================================================
// UPDATE TYPES (Output from process_instruction, input to process_update)
// ============================================================================

/// Update produced by job creation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateJobUpdateV1 {
    pub job_id: pallas::Base,
    pub employer: PublicKey,
    pub title_hash: pallas::Base,
    pub specification_hash: pallas::Base,
    pub delivery_type: DeliveryType,
    pub total_payment: u64,
    pub milestone_count: u32,
    pub deadline_block: u64,
    pub milestones: Vec<Milestone>,
}

/// Update produced by job acceptance
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AcceptJobUpdateV1 {
    pub job_id: pallas::Base,
    pub worker: PublicKey,
}

/// Update produced by deliverable submission
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SubmitDeliverableUpdateV1 {
    pub job_id: pallas::Base,
    pub milestone_index: u32,
    pub deliverable_hash: pallas::Base,
    pub submitted_at_block: u64,
}

/// Update produced by deliverable confirmation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConfirmDeliverableUpdateV1 {
    pub job_id: pallas::Base,
    pub milestone_index: u32,
    pub payment_released: u64,
}

/// Update produced by dispute
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DisputeUpdateV1 {
    pub job_id: pallas::Base,
    pub dispute_reason_hash: pallas::Base,
}

/// Update produced by cancellation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelJobUpdateV1 {
    pub job_id: pallas::Base,
    pub refund_amount: u64,
}

/// Update produced by refund
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RefundUpdateV1 {
    pub job_id: pallas::Base,
    pub refund_amount: u64,
}