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

//! DAO-Escrow contract data structures
//!
//! ## Overview
//!
//! This contract combines DAO governance with escrow mechanics:
//! - DAO-Escrow is identified by a `bulla` (commitment)
//! - Members pay premiums into the endowment pool
//! - Claims are proposed, voted on, and if approved, execute like escrow claims

use darkfi_sdk::{
    crypto::{poseidon_hash, BaseBlind, MerkleNode, PublicKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// DAO-Escrow unique identifier (hash of parameters)
pub type DaoEscrowBulla = pallas::Base;

/// Claim identifier
pub type ClaimId = pallas::Base;

/// Vote identifier
pub type VoteId = pallas::Base;

// ============================================================================
// DAO-ESCROW CONFIGURATION
// ============================================================================

/// Represents a DAO-Escrow instance
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoEscrow {
    /// Bulla (unique identifier)
    pub bulla: DaoEscrowBulla,
    /// Owner/creator public key
    pub owner_pubkey: PublicKey,
    /// Governance token ID for voting
    pub gov_token_id: pallas::Base,
    /// Minimum governance tokens to propose a claim
    pub proposer_limit: u64,
    /// Quorum: minimum total vote weight needed for valid result
    pub quorum: u64,
    /// Early execution quorum (for fast-track)
    pub early_exec_quorum: u64,
    /// Approval ratio: yes_votes / total_votes needed to pass
    pub approval_ratio_quot: u64,
    pub approval_ratio_base: u64,
    /// Premium rate: percentage of member balance to pay as premium
    pub premium_rate_quot: u64,
    pub premium_rate_base: u64,
    /// Maximum claim amount as ratio of total endowment
    pub max_claim_ratio_quot: u64,
    pub max_claim_ratio_base: u64,
    /// Claim voting window (in blocks)
    pub claim_voting_window: u64,
    /// Claim execution window after approval (in blocks)
    pub claim_execution_window: u64,
    /// Total endowment value
    pub total_endowment: u64,
    /// Number of members
    pub member_count: u64,
    /// Creation block
    pub created_at: u64,
    /// Bulla blind factor
    pub bulla_blind: BaseBlind,
}

impl DaoEscrow {
    /// Derive the bulla (unique identifier) from parameters
    pub fn derive_bulla(
        owner_pubkey: &PublicKey,
        gov_token_id: pallas::Base,
        proposer_limit: u64,
        quorum: u64,
        early_exec_quorum: u64,
        approval_ratio_quot: u64,
        approval_ratio_base: u64,
        premium_rate_quot: u64,
        premium_rate_base: u64,
        max_claim_ratio_quot: u64,
        max_claim_ratio_base: u64,
        bulla_blind: BaseBlind,
    ) -> DaoEscrowBulla {
        let (ox, oy) = owner_pubkey.xy();
        poseidon_hash([
            ox,
            oy,
            gov_token_id,
            pallas::Base::from(proposer_limit),
            pallas::Base::from(quorum),
            pallas::Base::from(early_exec_quorum),
            pallas::Base::from(approval_ratio_quot),
            pallas::Base::from(approval_ratio_base),
            pallas::Base::from(premium_rate_quot),
            pallas::Base::from(premium_rate_base),
            pallas::Base::from(max_claim_ratio_quot),
            pallas::Base::from(max_claim_ratio_base),
            bulla_blind.inner(),
        ])
    }
}

// ============================================================================
// CLAIM STATE MACHINE
// ============================================================================

/// Represents the state of a claim
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum ClaimState {
    /// Claim proposed, voting in progress
    Pending = 0,
    /// Claim approved by DAO vote
    Approved = 1,
    /// Claim rejected by DAO vote
    Rejected = 2,
    /// Claim executed, funds released
    Executed = 3,
    /// Claim cancelled by proposer
    Cancelled = 4,
    /// Claim expired (voting window passed)
    Expired = 5,
}

impl TryFrom<u8> for ClaimState {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Approved),
            2 => Ok(Self::Rejected),
            3 => Ok(Self::Executed),
            4 => Ok(Self::Cancelled),
            5 => Ok(Self::Expired),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// CLAIM
// ============================================================================

/// A claim against the endowment
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Claim {
    /// Unique claim identifier
    pub id: ClaimId,
    /// DAO-Escrow bulla this claim belongs to
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Proposer's public key
    pub proposer_pubkey: PublicKey,
    /// Claimed value amount
    pub value: u64,
    /// Token ID for the claim
    pub token_id: pallas::Base,
    /// Human-readable claim description (hashed)
    pub description_hash: pallas::Base,
    /// Current state
    pub state: ClaimState,
    /// Yes votes accumulated
    pub yes_votes: u64,
    /// No votes accumulated
    pub no_votes: u64,
    /// Total votes (yes + no)
    pub total_votes: u64,
    /// Block when voting ends
    pub voting_deadline: u64,
    /// Block when execution deadline ends (after approval)
    pub execution_deadline: Option<u64>,
    /// Value commitment for the payout
    pub value_commit: pallas::Point,
    /// Recipient public key for the payout
    pub recipient_pubkey: PublicKey,
    /// Created at block
    pub created_at: u64,
}

impl Claim {
    /// Check if claim has reached quorum and approval ratio
    pub fn is_approved(&self) -> bool {
        // Note: This is a simplified check. Real implementation would use
        // the cross-multiplication pattern: yes_votes * base < quotient * total_votes
        self.total_votes >= self.yes_votes && self.yes_votes > 0
    }
}

/// Vote type
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum VoteType {
    Yes = 0,
    No = 1,
}

// ============================================================================
// PREMIUM TRACKING
// ============================================================================

/// Premium payment record
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PremiumPayment {
    /// Member's public key
    pub member_pubkey: PublicKey,
    /// Amount paid
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Premium period this payment covers
    pub period: u64,
    /// Block when paid
    pub paid_at: u64,
}

/// Member's premium balance
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MemberBalance {
    /// Member's public key
    pub member_pubkey: PublicKey,
    /// Total premiums paid
    pub total_premiums: u64,
    /// Current period
    pub current_period: u64,
    /// Last payment block
    pub last_payment_block: u64,
}

// ============================================================================
// PARAMETERS (for contract calls)
// ============================================================================

/// Parameters for `DaoEscrow::InitializeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParamsV1 {
    /// Owner's public key
    pub owner_pubkey: PublicKey,
    /// Governance token ID
    pub gov_token_id: pallas::Base,
    /// Minimum tokens to propose
    pub proposer_limit: u64,
    /// Quorum for voting
    pub quorum: u64,
    /// Early execution quorum
    pub early_exec_quorum: u64,
    /// Approval ratio (quot/base)
    pub approval_ratio_quot: u64,
    pub approval_ratio_base: u64,
    /// Premium rate (quot/base of member balance)
    pub premium_rate_quot: u64,
    pub premium_rate_base: u64,
    /// Maximum claim ratio (quot/base of endowment)
    pub max_claim_ratio_quot: u64,
    pub max_claim_ratio_base: u64,
    /// Claim voting window in blocks
    pub claim_voting_window: u64,
    /// Claim execution window in blocks
    pub claim_execution_window: u64,
}

/// State update for `DaoEscrow::InitializeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeUpdateV1 {
    /// The created DAO-Escrow bulla
    pub bulla: DaoEscrowBulla,
}

/// Parameters for `DaoEscrow::UpdateV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateParamsV1 {
    /// DAO-Escrow bulla
    pub bulla: DaoEscrowBulla,
    /// New governance params (optional, only changed values)
    pub new_proposer_limit: Option<u64>,
    pub new_quorum: Option<u64>,
    pub new_early_exec_quorum: Option<u64>,
    pub new_approval_ratio_quot: Option<u64>,
    pub new_approval_ratio_base: Option<u64>,
    pub new_premium_rate_quot: Option<u64>,
    pub new_premium_rate_base: Option<u64>,
    pub new_max_claim_ratio_quot: Option<u64>,
    pub new_max_claim_ratio_base: Option<u64>,
}

/// State update for `DaoEscrow::UpdateV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateUpdateV1 {
    /// Updated DAO-Escrow bulla
    pub bulla: DaoEscrowBulla,
}

/// Parameters for `DaoEscrow::PayPremiumV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PayPremiumParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Member's value commitment
    pub value_commit: pallas::Point,
    /// Premium amount being paid
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Premium period
    pub period: u64,
}

/// State update for `DaoEscrow::PayPremiumV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PayPremiumUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Updated total endowment
    pub total_endowment: u64,
    /// Updated member count
    pub member_count: u64,
}

/// Parameters for `DaoEscrow::ProposeClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProposeClaimParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim value
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Description hash
    pub description_hash: pallas::Base,
    /// Recipient public key for payout
    pub recipient_pubkey: PublicKey,
    /// Merkle proof of proposer's governance tokens
    pub merkle_proof: Vec<pallas::Base>,
    /// Merkle root for governance tokens
    pub merkle_root: MerkleNode,
    /// Value commitment for the payout
    pub value_commit: pallas::Point,
}

/// State update for `DaoEscrow::ProposeClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProposeClaimUpdateV1 {
    /// The created claim ID
    pub claim_id: ClaimId,
}

/// Parameters for `DaoEscrow::VoteClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VoteClaimParamsV1 {
    /// Claim ID
    pub claim_id: ClaimId,
    /// Vote type (yes/no)
    pub vote: VoteType,
    /// Voter's governance token commitment
    pub vote_commit: pallas::Point,
    /// Vote nullifier
    pub vote_nullifier: pallas::Base,
    /// Signature
    pub signature_public: PublicKey,
}

/// State update for `DaoEscrow::VoteClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VoteClaimUpdateV1 {
    /// Updated claim ID
    pub claim_id: ClaimId,
    /// New yes votes
    pub yes_votes: u64,
    /// New no votes
    pub no_votes: u64,
    /// New total votes
    pub total_votes: u64,
    /// New state (if changed)
    pub new_state: Option<ClaimState>,
}

/// Parameters for `DaoEscrow::ExecuteClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteClaimParamsV1 {
    /// Claim ID
    pub claim_id: ClaimId,
}

/// State update for `DaoEscrow::ExecuteClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteClaimUpdateV1 {
    /// Claim ID
    pub claim_id: ClaimId,
    /// Released value
    pub released_value: u64,
    /// Recipient
    pub recipient_pubkey: PublicKey,
    /// Updated total endowment
    pub total_endowment: u64,
}

/// Parameters for `DaoEscrow::CancelClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelClaimParamsV1 {
    /// Claim ID
    pub claim_id: ClaimId,
    /// Proposer's secret (to verify ownership)
    pub proposer_secret: pallas::Base,
}

/// State update for `DaoEscrow::CancelClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelClaimUpdateV1 {
    /// Cancelled claim ID
    pub claim_id: ClaimId,
}

/// Parameters for `DaoEscrow::WithdrawV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Amount to withdraw
    pub value: u64,
    /// Recipient
    pub recipient_pubkey: PublicKey,
}

/// State update for `DaoEscrow::WithdrawV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Withdrawn amount
    pub value: u64,
    /// Updated total endowment
    pub total_endowment: u64,
}
