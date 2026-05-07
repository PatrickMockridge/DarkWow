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

//! Insurance Contract Data Structures
//!
//! ZK insurance contract with full policy lifecycle management.
//!
//! ## Policy State Machine
//!
//! ```text
//! Created -> Active -> Expired
//!    |          |
//!    |          +-> Claimed -> Approved -> Paid
//!    |                         |
//!    |                         +-> Rejected (back to Active)
//!    +-> Cancelled
//! ```
//!
//! ## Key Calculation
//!
//! Claim approval uses base_div for privacy-preserving calculation:
//! `approved_amount = verified_loss * coverage_ratio / 10000`

use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// POLICY STATE (State machine)
// ============================================================================

/// Policy state in the lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum PolicyStatus {
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

impl TryFrom<u8> for PolicyStatus {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Created),
            1 => Ok(Self::Active),
            2 => Ok(Self::Expired),
            3 => Ok(Self::Cancelled),
            4 => Ok(Self::Claimed),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
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

impl TryFrom<u8> for ClaimStatus {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Approved),
            2 => Ok(Self::Rejected),
            3 => Ok(Self::Paid),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// POLICY (Core data structure)
// ============================================================================

/// An insurance policy
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Policy {
    /// Unique policy identifier (Poseidon hash)
    pub id: pallas::Base,
    /// Policyholder's public key
    pub policyholder: PublicKey,
    /// Hash of personal details (stored off-chain)
    pub details_hash: pallas::Base,
    /// Coverage amount (payout on claim)
    pub coverage_amount: u64,
    /// Premium amount paid
    pub premium: u64,
    /// Coverage ratio in basis points (e.g., 8000 = 80%)
    pub coverage_ratio: u64,
    /// Token for payment
    pub payment_token: pallas::Base,
    /// Coverage start block
    pub start_block: u64,
    /// Coverage end block
    pub end_block: u64,
    /// Current policy status
    pub status: PolicyStatus,
    /// Total claims filed against this policy
    pub total_claims: u32,
    /// Total payouts made
    pub total_payouts: u64,
}

impl Policy {
    /// Derive the policy ID from policy details
    #[allow(dead_code)]
    pub fn derive_id(
        policyholder: &PublicKey,
        details_hash: pallas::Base,
        coverage_amount: u64,
        premium: u64,
        start_block: u64,
        end_block: u64,
    ) -> pallas::Base {
        let (px, py) = policyholder.xy();
        poseidon_hash([
            px,
            py,
            details_hash,
            pallas::Base::from(coverage_amount),
            pallas::Base::from(premium),
            pallas::Base::from(start_block),
            pallas::Base::from(end_block),
        ])
    }
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
    pub details_hash: pallas::Base,
    /// Verified loss amount (assessed by oracle or DAO)
    pub verified_loss: u64,
    /// Approved payout amount (calculated from verified_loss * coverage_ratio / 10000)
    pub approved_amount: u64,
    /// Current claim status
    pub status: ClaimStatus,
    /// Block when claim was filed
    pub filed_at_block: u64,
    /// Block when claim was processed
    pub processed_at_block: Option<u64>,
}

impl Claim {
    /// Derive the claim ID from claim parameters
    #[allow(dead_code)]
    pub fn derive_id(policy_id: pallas::Base, claim_amount: u64, details_hash: pallas::Base, current_block: u64) -> pallas::Base {
        poseidon_hash([
            policy_id,
            pallas::Base::from(claim_amount),
            details_hash,
            pallas::Base::from(current_block),
        ])
    }
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
    pub premium: u64,
    /// Token for payment
    pub payment_token: pallas::Base,
    /// Coverage start block
    pub start_block: u64,
    /// Coverage end block
    pub end_block: u64,
}

/// Parameters for activating a policy (after premium paid)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ActivatePolicyParamsV1 {
    /// Policy ID
    pub policy_id: pallas::Base,
    /// Caller (should be pool manager or automatic)
    pub caller: PublicKey,
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
}

/// Parameters for rejecting a claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RejectClaimParamsV1 {
    /// Claim ID
    pub claim_id: pallas::Base,
}

/// Parameters for paying out a claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PayClaimParamsV1 {
    /// Claim ID
    pub claim_id: pallas::Base,
    /// Payout amount
    pub payout_amount: u64,
}

/// Parameters for cancelling a policy
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelPolicyParamsV1 {
    /// Policy ID
    pub policy_id: pallas::Base,
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
    pub premium: u64,
    pub coverage_ratio: u64,
    pub payment_token: pallas::Base,
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
    pub policy_id: pallas::Base,
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