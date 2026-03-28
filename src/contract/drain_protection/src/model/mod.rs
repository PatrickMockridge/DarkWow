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

//! DrainProtection contract data structures
//!
//! This contract provides governance-level protections for endowment/treasury funds:
//! - Rate limiting per block
//! - 2/3 vote thresholds for large withdrawals
//! - Lock/unlock emergency controls
//! - Member exit with haircut

use darkfi_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Unique identifier for the protected fund (derived from DAO-Escrow bulla)
pub type FundId = pallas::Base;

/// Member's contribution weight (block-height-adjusted)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MemberWeight {
    /// Raw contribution amount
    pub contribution: u64,
    /// Block height when contribution was made
    pub deposited_at: u64,
    /// Weight multiplier (longer deposit time = higher weight)
    pub weight_multiplier: u64,
}

impl MemberWeight {
    /// Compute the effective weight for exit calculations
    pub fn effective_weight(&self, current_block: u64) -> u64 {
        let blocks_held = current_block.saturating_sub(self.deposited_at);
        // Weight increases with time held, capped at 3x after ~1 year
        let time_multiplier = 1_000 + (blocks_held / 10_000).min(2_000);
        self.contribution * time_multiplier / 1_000
    }
}

/// Represents a protected fund with drain controls
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProtectedFund {
    /// Unique fund identifier
    pub id: FundId,
    /// Total funds under protection
    pub total_funds: u64,
    /// Spend authority (who can propose withdrawals)
    pub spend_authority: PublicKey,
    /// Current lock state
    pub lock_state: LockState,
    /// Rate limit configuration
    pub rate_limit: RateLimit,
    /// Vote thresholds
    pub thresholds: VoteThresholds,
    /// Members and their weights
    pub members: Vec<MemberWeight>,
    /// Emergency lock expiry block
    pub lock_expires_at: u64,
    /// Spend authority change timelock
    pub authority_change_timelock: u64,
    /// Block height when fund was created
    pub created_at: u64,
}

/// Lock state of the fund
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum LockState {
    /// Funds are unlocked and available
    Unlocked = 0,
    /// Funds are locked (emergency state)
    Locked = 1,
}

/// Rate limiting configuration
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RateLimit {
    /// Base rate: percentage of total per 1000 blocks (e.g., 10 = 0.1%)
    pub base_rate_bps: u64,
    /// Blocks over which rate is averaged
    pub averaging_window_blocks: u64,
    /// Transfers exceeding base rate require vote
    pub vote_required_above_bps: u64,
}

impl Default for RateLimit {
    fn default() -> Self {
        // Default: 1% per 1000 blocks, requires vote above that
        Self {
            base_rate_bps: 100,       // 1%
            averaging_window_blocks: 1000,
            vote_required_above_bps: 100,
        }
    }
}

/// Vote threshold configuration
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VoteThresholds {
    /// Threshold for large withdrawals (e.g., 667 = 66.7% = 2/3)
    pub large_withdrawal_thresh: u64,
    /// Threshold for lock/unlock
    pub lock_unlock_thresh: u64,
    /// Threshold for spend authority changes
    pub authority_change_thresh: u64,
    /// Minimum quorum participation required
    pub quorum_min_bps: u64,
}

impl Default for VoteThresholds {
    fn default() -> Self {
        Self {
            large_withdrawal_thresh: 667, // 66.7% = 2/3
            lock_unlock_thresh: 667,       // 66.7% = 2/3
            authority_change_thresh: 667,   // 66.7% = 2/3
            quorum_min_bps: 500,           // 50% quorum
        }
    }
}

/// A pending vote proposal
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VoteProposal {
    /// Unique proposal ID
    pub id: pallas::Base,
    /// Type of action being voted on
    pub action: VoteAction,
    /// Block when voting started
    pub started_at: u64,
    /// Block when voting ends
    pub ends_at: u64,
    /// Votes received (yes)
    pub yes_votes: u64,
    /// Votes received (no)
    pub no_votes: u64,
    /// Whether vote has concluded
    pub concluded: bool,
}

/// Possible vote actions
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum VoteAction {
    /// Large withdrawal requiring vote
    LargeWithdrawal { amount: u64, recipient: PublicKey },
    /// Lock the funds
    LockFunds,
    /// Unlock the funds
    UnlockFunds,
    /// Change spend authority
    ChangeSpendAuthority { new_authority: PublicKey },
    /// Renew emergency lock
    RenewLock,
}

/// Record of a fund transfer for rate limiting
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferRecord {
    /// Block height of transfer
    pub block: u64,
    /// Amount transferred
    pub amount: u64,
}

/// Exit request from a member
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExitRequest {
    /// Request ID
    pub id: pallas::Base,
    /// Member public key
    pub member_pubkey: PublicKey,
    /// Contribution weight at time of request
    pub weight: u64,
    /// Requested exit value (before haircut)
    pub requested_value: u64,
    /// Haircut applied
    pub haircut_bps: u64,
    /// Actual payout (after haircut)
    pub payout_value: u64,
    /// Block when request was made
    pub requested_at: u64,
    /// Whether exit has been processed
    pub processed: bool,
}

// ============================================================================
// Contract Function Parameters
// ============================================================================

/// Parameters for `DrainProtection::InitializeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParamsV1 {
    /// Fund identifier (from DAO-Escrow)
    pub fund_id: FundId,
    /// Initial spend authority
    pub spend_authority: PublicKey,
    /// Associated DAO-Escrow bulla
    pub dao_escrow_bulla: pallas::Base,
}

/// State update for `DrainProtection::InitializeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeUpdateV1 {
    pub fund_id: FundId,
}

/// Parameters for `DrainProtection::ProposeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProposeParamsV1 {
    /// The proposed action
    pub action: VoteAction,
    /// Prover's public key
    pub prover_pubkey: PublicKey,
    /// Vote period in blocks
    pub vote_period_blocks: u64,
    /// ZK proof
    pub proof: Vec<u8>,
}

/// State update for `DrainProtection::ProposeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProposeUpdateV1 {
    pub proposal_id: pallas::Base,
}

/// Parameters for `DrainProtection::VoteV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VoteParamsV1 {
    /// Proposal ID to vote on
    pub proposal_id: pallas::Base,
    /// Voter's public key
    pub voter_pubkey: PublicKey,
    /// Vote choice (true = yes, false = no)
    pub vote: bool,
    /// Signature
    pub signature: pallas::Base,
}

/// State update for `DrainProtection::VoteV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VoteUpdateV1 {
    pub proposal_id: pallas::Base,
    pub yes_votes: u64,
    pub no_votes: u64,
}

/// Parameters for `DrainProtection::ExecuteV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteParamsV1 {
    /// Proposal ID to execute
    pub proposal_id: pallas::Base,
    /// Executor signature
    pub signature: pallas::Base,
}

/// State update for `DrainProtection::ExecuteV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteUpdateV1 {
    pub proposal_id: pallas::Base,
    pub action_executed: VoteAction,
}

/// Parameters for `DrainProtection::ExitV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExitParamsV1 {
    /// Member public key
    pub member_pubkey: PublicKey,
    /// Member's contribution proof
    pub contribution_weight: u64,
    /// Current block height
    pub current_block: u64,
    /// ZK proof of membership
    pub proof: Vec<u8>,
}

/// State update for `DrainProtection::ExitV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExitUpdateV1 {
    pub exit_id: pallas::Base,
    pub member_pubkey: PublicKey,
    pub payout_value: u64,
    pub haircut_collected: u64,
}

/// Parameters for `DrainProtection::TransferV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferParamsV1 {
    /// Amount to transfer
    pub amount: u64,
    /// Recipient
    pub recipient: PublicKey,
    /// Spend authority signature
    pub signature: pallas::Base,
    /// Whether this exceeds rate limit (requires prior vote)
    pub exceeds_rate_limit: bool,
    /// If exceeds_rate_limit, proposal ID that was voted
    pub vote_proposal_id: Option<pallas::Base>,
}

/// State update for `DrainProtection::TransferV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferUpdateV1 {
    pub amount: u64,
    pub recipient: PublicKey,
    pub rate_limited: bool,
}

/// Parameters for `DrainProtection::LockV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LockParamsV1 {
    /// Lock duration in blocks
    pub duration_blocks: u64,
    /// Signature from spend authority
    pub signature: pallas::Base,
}

/// State update for `DrainProtection::LockV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LockUpdateV1 {
    pub locked_until: u64,
}

/// Parameters for `DrainProtection::UnlockV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnlockParamsV1 {
    /// Signature from spend authority
    pub signature: pallas::Base,
}

/// State update for `DrainProtection::UnlockV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnlockUpdateV1 {
    pub unlocked_at: u64,
}

/// Parameters for `DrainProtection::UpdateConfigV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigParamsV1 {
    /// New rate limit (optional)
    pub rate_limit: Option<RateLimit>,
    /// New vote thresholds (optional)
    pub thresholds: Option<VoteThresholds>,
    /// Spend authority (optional, subject to timelock)
    pub new_spend_authority: Option<PublicKey>,
}

/// State update for `DrainProtection::UpdateConfigV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigUpdateV1 {
    pub authority_change_timelock: Option<u64>,
}