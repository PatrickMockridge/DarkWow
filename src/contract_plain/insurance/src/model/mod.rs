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

//! Plain Insurance Contract Model
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
//! | Policy commitment | ZK (Pedersen) | Privacy-preserving |
//! | Premium calculation | Native Rust | Needs `base_div` (not in ZK) |
//! | Claims verification | Hybrid | ZK for sound parts, plain for complex |

use darkfi_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use darkfi_sdk::crypto::schnorr::Signature;
use darkfi_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// POLICY STATE (State machine - visible on-chain)
// ============================================================================

/// Policy state in the state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum PolicyState {
    /// Policy created, awaiting premium payment
    Created = 0,
    /// Policy active, coverage period started
    Active = 1,
    /// Coverage period expired
    Expired = 2,
    /// Policy cancelled by holder
    Cancelled = 3,
    /// Claim filed, under review
    Claimed = 4,
}

/// Claim status
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum ClaimStatus {
    /// Claim filed, under review
    Pending = 0,
    /// Claim approved, awaiting payout
    Approved = 1,
    /// Claim rejected
    Rejected = 2,
    /// Claim paid out
    Paid = 3,
}

// ============================================================================
// POLICY (Plain - all fields visible except personal details)
// ============================================================================

/// An insurance policy
/// PRIVACY NOTICE: Most fields are PUBLIC in plain version.
/// Actual personal details (health, property) are NOT stored on-chain.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Policy {
    /// Unique policy identifier (Poseidon hash)
    pub id: pallas::Base,
    /// Policyholder's public key
    pub policyholder: PublicKey,
    /// Hash of personal details (stored off-chain)
    /// PRIVACY NOTICE: Actual details are off-chain, only hash is public
    pub details_hash: pallas::Base,
    /// Coverage amount (payout on claim)
    pub coverage_amount: u64,
    /// Premium amount paid
    pub premium_paid: u64,
    /// Token for payment
    pub payment_token: pallas::Base,
    /// Coverage start block
    pub start_block: u64,
    /// Coverage end block
    pub end_block: u64,
    /// Current policy state
    pub state: PolicyState,
    /// Total claims filed against this policy
    pub total_claims: u32,
    /// Total payouts made
    pub total_payouts: u64,
    /// Signature from policyholder for authorization
    /// ZK: Schnorr signature verified in ZK
    pub policyholder_signature: Option<Signature>,
}

// ============================================================================
// CLAIM (For insurance claims)
// ============================================================================

/// An insurance claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Claim {
    /// Unique claim identifier (Poseidon hash)
    pub id: pallas::Base,
    /// Policy ID this claim belongs to
    pub policy_id: pallas::Base,
    /// Claim amount requested
    pub claim_amount: u64,
    /// Hash of claim details/reason
    /// PRIVACY NOTICE: Actual claim details are off-chain
    pub details_hash: pallas::Base,
    /// Verified loss amount (assessed by oracle or DAO)
    pub verified_loss: u64,
    /// Current claim status
    pub status: ClaimStatus,
    /// Block when claim was filed
    pub filed_at_block: u64,
    /// Block when claim was processed
    pub processed_at_block: Option<u64>,
}

// ============================================================================
// PARAMETERS (Input types for contract calls)
// ============================================================================

/// Parameters for creating a new policy
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreatePolicyParamsV1 {
    /// Policyholder's public key
    pub policyholder: PublicKey,
    /// Hash of personal details
    pub details_hash: pallas::Base,
    /// Coverage amount requested
    pub coverage_amount: u64,
    /// Premium amount being paid
    pub premium_amount: u64,
    /// Token for payment
    pub payment_token: pallas::Base,
    /// Coverage start block
    pub start_block: u64,
    /// Coverage end block
    pub end_block: u64,
    /// Policyholder's signature over policy params
    /// ZK: Schnorr signature verified in ZK to constrain policyholder
    pub signature: Signature,
}

/// Parameters for activating a policy (after premium paid)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ActivatePolicyParamsV1 {
    /// Policy ID
    pub policy_id: pallas::Base,
    /// Caller (should be pool manager or automatic)
    pub caller: PublicKey,
    /// Signature over activation
    pub signature: Signature,
}

/// Parameters for filing a claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FileClaimParamsV1 {
    /// Policy ID
    pub policy_id: pallas::Base,
    /// Claim amount
    pub claim_amount: u64,
    /// Hash of claim details/reason
    pub details_hash: pallas::Base,
    /// Policyholder's signature over claim
    /// ZK: Schnorr signature verified in ZK
    pub signature: Signature,
}

/// Parameters for approving a claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ApproveClaimParamsV1 {
    /// Claim ID
    pub claim_id: pallas::Base,
    /// Verified loss amount
    pub verified_loss: u64,
    /// Coverage ratio (e.g., 8000 = 80%)
    pub coverage_ratio: u64,
    /// Approver's signature
    pub signature: Signature,
}

/// Parameters for rejecting a claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RejectClaimParamsV1 {
    /// Claim ID
    pub claim_id: pallas::Base,
    /// Rejector's signature (DAO or oracle)
    pub signature: Signature,
}

/// Parameters for paying out a claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PayClaimParamsV1 {
    /// Claim ID
    pub claim_id: pallas::Base,
    /// Payout amount
    pub payout_amount: u64,
    /// Callers signature
    pub signature: Signature,
}

/// Parameters for cancelling a policy
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelPolicyParamsV1 {
    /// Policy ID
    pub policy_id: pallas::Base,
    /// Policyholder's signature over cancellation
    pub signature: Signature,
}

// ============================================================================
// UPDATE TYPES (Output from process_instruction, input to process_update)
// ============================================================================

/// Update produced by policy creation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreatePolicyUpdateV1 {
    pub policy_id: pallas::Base,
    pub policyholder: PublicKey,
    pub details_hash: pallas::Base,
    pub coverage_amount: u64,
    pub premium_paid: u64,
    pub start_block: u64,
    pub end_block: u64,
}

/// Update produced by policy activation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ActivatePolicyUpdateV1 {
    pub policy_id: pallas::Base,
}

/// Update produced by claim filing
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FileClaimUpdateV1 {
    pub claim_id: pallas::Base,
    pub policy_id: pallas::Base,
    pub claim_amount: u64,
    pub details_hash: pallas::Base,
    pub filed_at_block: u64,
}

/// Update produced by claim approval
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ApproveClaimUpdateV1 {
    pub claim_id: pallas::Base,
    pub policy_id: pallas::Base,
    pub verified_loss: u64,
    pub coverage_ratio: u64,
    pub approved_amount: u64,
}

/// Update produced by claim rejection
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RejectClaimUpdateV1 {
    pub claim_id: pallas::Base,
}

/// Update produced by claim payout
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PayClaimUpdateV1 {
    pub claim_id: pallas::Base,
    pub payout_amount: u64,
}

/// Update produced by policy cancellation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelPolicyUpdateV1 {
    pub policy_id: pallas::Base,
    pub refund_amount: u64,
}