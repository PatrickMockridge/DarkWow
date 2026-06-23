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

//! DrainProtection contract data structures
//!
//! This contract provides governance-level protections for endowment/treasury funds:
//! - Rate limiting per block
//! - 2/3 vote thresholds for large withdrawals
//! - Lock/unlock emergency controls
//! - Member exit with haircut

use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

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
    pub version: u8,
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Unique fund identifier
    pub id: FundId,
    /// Total funds under protection
    pub total_funds: u64,
    /// Spend authority (who can propose withdrawals)
    pub spend_authority: PublicKey,
    /// Current lock state
    pub lock_state: LockState,
    /// Legacy rate limit configuration (used if graduated_tiers disabled)
    pub rate_limit: RateLimit,
    /// MultiSig group ID for per-action threshold voting (replaces VoteThresholds)
    pub multisig_group_id: pallas::Base,
    /// Purse instance ID for Pedersen-committed balance tracking (replaces total_funds)
    pub purse_id: pallas::Base,
    /// Comprehensive drain protection config (all optional features)
    pub drain_config: DrainConfig,
    /// Members and their weights
    pub members: Vec<MemberWeight>,
    /// Emergency lock expiry block
    pub lock_expires_at: u64,
    /// Spend authority change timelock
    pub authority_change_timelock: u64,
    /// Block height when fund was created
    pub created_at: u64,
    // ─────────────────────────────────────────────────────────────────
    // OPTIONAL FEATURE STATE
    // ─────────────────────────────────────────────────────────────────
    /// Exit queue state (if exit_queue enabled)
    pub exit_queue_state: Vec<ExitQueueEntry>,
    /// Circuit breaker state (if circuit_breaker enabled)
    pub circuit_breaker_state: Option<CircuitBreakerState>,
    /// Dead man's switch state (if dead_mans_switch enabled)
    pub dead_mans_switch_state: Option<DeadMansSwitchState>,
    /// Current no-loss reserve balance (computed from total_funds and reserve_bps)
    pub no_loss_reserve_balance: u64,
    /// Pending observation period proposals
    pub observation_pending: Vec<ObservationPending>,
}

/// Pending observation period entry
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ObservationPending {
    /// Proposal ID
    pub proposal_id: pallas::Base,
    /// Amount being withdrawn
    pub amount: u64,
    /// Observation ends at block
    pub observation_ends_at: u64,
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

/// Record of a fund transfer for rate limiting
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferRecord {
    pub version: u8,
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
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Fund identifier (from DAO-Escrow)
    pub fund_id: FundId,
    /// Initial spend authority
    pub spend_authority: PublicKey,
    /// Associated DAO-Escrow bulla
    pub dao_escrow_bulla: pallas::Base,
    /// Drain protection configuration (all features optional)
    pub drain_config: DrainConfig,
}

/// State update for `DrainProtection::InitializeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeUpdateV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    pub fund_id: FundId,
}

/// Parameters for `DrainProtection::ProposeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProposeParamsV1 {
    /// MultiSig message hash (the action being proposed for approval)
    pub message_hash: pallas::Base,
    /// MultiSig group ID this proposal targets
    pub multisig_group_id: pallas::Base,
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
    pub action: pallas::Base,
}

/// Parameters for `DrainProtection::ExitV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExitParamsV1 {
    /// Fund to exit from
    pub fund_id: FundId,
    /// Member public key
    pub member_pubkey: PublicKey,
    /// Member's contribution proof
    pub contribution_weight: u64,
    /// Current block height
    pub current_block: u64,
    /// DAO-Escrow bulla identifier (public input for ZK proof)
    pub dao_escrow_bulla: pallas::Base,
    /// DAO-Escrow membership note (public input for ZK proof)
    pub dao_membership_note: pallas::Base,
    /// Effective weight after time multiplier (public input for ZK proof)
    pub effective_weight: pallas::Base,
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
    /// Fund to transfer from
    pub fund_id: FundId,
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
    /// Fund to lock
    pub fund_id: FundId,
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
    /// Fund to unlock
    pub fund_id: FundId,
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
    /// Fund to update
    pub fund_id: FundId,
    /// New rate limit (optional)
    pub rate_limit: Option<RateLimit>,
    /// New MultiSig group ID for governance (optional, replaces VoteThresholds)
    pub multisig_group_id: Option<pallas::Base>,
    /// Spend authority (optional, subject to timelock)
    pub new_spend_authority: Option<PublicKey>,
}

/// State update for `DrainProtection::UpdateConfigV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigUpdateV1 {
    pub authority_change_timelock: Option<u64>,
}

// ============================================================================
// DRAIN PROTECTION CONFIG (All Optional Best Practices)
// ============================================================================

/// Comprehensive drain protection configuration.
/// All features are OPTIONAL and default to disabled.
/// Enable features based on your risk tolerance and needs.
///
/// # Features
///
/// | Feature | Purpose | Risk Mitigated |
/// |---------|---------|----------------|
/// | `graduated_tiers` | Multi-tier withdrawal limits | Prevents single-vote large drains |
/// | `exit_queue` | FCFS exit processing | Prevents bank-run cascades |
/// | `circuit_breaker` | Auto-pause on anomalous drain | Stops bleeding during attacks |
/// | `guardian_pause` | Multisig pause capability | Manual emergency stop |
/// | `observation_period` | Delay before large withdrawals | Gives members time to react |
/// | `split_proposals` | Split large proposals | Prevents single proposal drains |
/// | `no_loss_reserve` | 20% untouchable reserve | Always have insurance funds |
/// | `dead_mans_switch` | Auto-protocol on inactivity | Protects against abandonment |
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DrainConfig {
    // ─────────────────────────────────────────────────────────────────
    // MULTISIG GOVERNANCE — replaces graduated_tiers + guardian_pause
    // ─────────────────────────────────────────────────────────────────
    /// MultiSig group ID for guardian pause/unpause authorization.
    /// Guardians call MultiSig::SignV1; when threshold met, FinalizeV1
    /// produces an approval_commit that authorizes pause/unpause.
    pub guardian_multisig_group_id: pallas::Base,

    // ─────────────────────────────────────────────────────────────────
    // EXIT QUEUE (FCFS)
    // ─────────────────────────────────────────────────────────────────
    /// Enable FCFS exit queue (prevents bank-run cascades)
    pub exit_queue: Option<ExitQueueConfig>,

    // ─────────────────────────────────────────────────────────────────
    // CIRCUIT BREAKER
    // ─────────────────────────────────────────────────────────────────
    /// Enable circuit breaker (auto-pause on anomalous activity)
    pub circuit_breaker: Option<CircuitBreakerConfig>,

    // ─────────────────────────────────────────────────────────────────
    // OBSERVATION PERIOD
    // ─────────────────────────────────────────────────────────────────
    /// Enable observation period for large withdrawals
    pub observation_period: Option<ObservationPeriodConfig>,

    // ─────────────────────────────────────────────────────────────────
    // SPLIT PROPOSALS
    // ─────────────────────────────────────────────────────────────────
    /// Enable mandatory proposal splitting for large withdrawals
    pub split_proposals: Option<SplitProposalsConfig>,

    // ─────────────────────────────────────────────────────────────────
    // NO-LOSS RESERVE
    // ─────────────────────────────────────────────────────────────────
    /// Enable no-loss reserve (percentage always kept as insurance)
    pub no_loss_reserve: Option<NoLossReserveConfig>,

    // ─────────────────────────────────────────────────────────────────
    // DEAD MAN'S SWITCH
    // ─────────────────────────────────────────────────────────────────
    /// Enable dead man's switch (auto-protocol on inactivity)
    pub dead_mans_switch: Option<DeadMansSwitchConfig>,
}

impl Default for DrainConfig {
    fn default() -> Self {
        Self {
            guardian_multisig_group_id: pallas::Base::zero(),
            exit_queue: Some(ExitQueueConfig::default()),
            circuit_breaker: Some(CircuitBreakerConfig::default()),
            observation_period: None,
            split_proposals: None,
            no_loss_reserve: None,
            dead_mans_switch: None,
        }
    }
}

/// Exit queue configuration (FCFS)
///
/// Prevents bank-run cascades by processing exits in order.
/// Max exit per epoch prevents draining more than TVL can handle.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExitQueueConfig {
    /// Maximum exits processed per epoch (bps of TVL, e.g., 1000 = 10%)
    pub max_exit_per_epoch_bps: u64,
    /// Epoch length in blocks
    pub epoch_blocks: u64,
    /// Minimum time in queue before processing (blocks)
    pub min_queue_blocks: u64,
    /// Enable强制顺序处理 (true = FCFS only, false = allow priority)
    pub force_fcfs: bool,
}

impl Default for ExitQueueConfig {
    fn default() -> Self {
        Self {
            // Max 10% TVL can exit per epoch
            max_exit_per_epoch_bps: 1000,
            // Epoch: 1 day (~600 blocks)
            epoch_blocks: 600,
            // Must wait at least 10 blocks in queue
            min_queue_blocks: 10,
            // Enforce strict FCFS
            force_fcfs: true,
        }
    }
}

/// Circuit breaker configuration
///
/// Auto-pauses withdrawals if drain rate exceeds threshold,
/// preventing continued bleeding during an attack.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CircuitBreakerConfig {
    /// Trigger threshold: if >X% drained in window, pause
    pub trigger_threshold_bps: u64,
    /// Window size in blocks to measure drain rate
    pub window_blocks: u64,
    /// Pause duration when triggered (blocks)
    pub pause_duration_blocks: u64,
    /// Auto-resume after pause (vs manual resume only)
    pub auto_resume: bool,
    /// Notify guardians when triggered
    pub notify_guardians: bool,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            // Trigger if >10% drained in window
            trigger_threshold_bps: 1000,
            // Measure over last 100 blocks
            window_blocks: 100,
            // Pause for 24 hours (~600 blocks)
            pause_duration_blocks: 600,
            // Manual resume required
            auto_resume: false,
            // Alert guardians
            notify_guardians: true,
        }
    }
}

/// Guardian multisig pause configuration
///
/// Designated watchers can pause withdrawals without full governance.
/// Observation period configuration
///
/// Large withdrawals must be publicly visible for a period
/// before execution, giving members time to react.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ObservationPeriodConfig {
    /// Minimum threshold to trigger observation (bps of TVL)
    pub threshold_bps: u64,
    /// Observation period in blocks
    pub observation_blocks: u64,
    /// Allow emergency bypass with higher quorum
    pub allow_emergency_bypass: bool,
    /// Emergency bypass quorum (bps)
    pub emergency_bypass_quorum_bps: u64,
}

impl Default for ObservationPeriodConfig {
    fn default() -> Self {
        Self {
            // Trigger for >5% TVL withdrawals
            threshold_bps: 500,
            // 48 hour observation
            observation_blocks: 48 * 6,
            // Allow bypass with 90% quorum
            allow_emergency_bypass: true,
            emergency_bypass_quorum_bps: 9000,
        }
    }
}

/// Split proposals configuration
///
/// Large proposals must be split into smaller chunks,
/// preventing single malicious proposal drains.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SplitProposalsConfig {
    /// Minimum threshold to trigger splitting (bps of TVL)
    pub threshold_bps: u64,
    /// Maximum chunk size (bps of TVL)
    pub max_chunk_bps: u64,
    /// Minimum time between chunks (blocks)
    pub chunk_delay_blocks: u64,
    /// Require separate vote for each chunk
    pub separate_vote_each_chunk: bool,
}

impl Default for SplitProposalsConfig {
    fn default() -> Self {
        Self {
            // Split if >10% TVL
            threshold_bps: 1000,
            // Max chunk: 10% TVL
            max_chunk_bps: 1000,
            // Wait 1 day between chunks
            chunk_delay_blocks: 600,
            // Each chunk needs separate vote
            separate_vote_each_chunk: true,
        }
    }
}

/// No-loss reserve configuration
///
/// A percentage of funds are never available for DAO governance
/// and serve as permanent insurance.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct NoLossReserveConfig {
    /// Reserve percentage (bps, e.g., 2000 = 20%)
    pub reserve_bps: u64,
    /// Who can authorize reserve usage (usually guardians or emergency vote only)
    pub reserve_spend_authority: ReserveSpendAuthority,
    /// Minimum reserve balance (absolute, prevents draining to near-zero)
    pub min_reserve_absolute: u64,
}

impl Default for NoLossReserveConfig {
    fn default() -> Self {
        Self {
            // 20% reserve
            reserve_bps: 2000,
            // Only emergency vote can spend reserve
            reserve_spend_authority: ReserveSpendAuthority::EmergencyVoteOnly,
            // Keep at least 1% TVL as absolute minimum
            min_reserve_absolute: 100,
        }
    }
}

/// Authority that can spend from no-loss reserve
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum ReserveSpendAuthority {
    /// Only emergency proposals with >90% quorum
    EmergencyVoteOnly,
    /// Guardian multisig only
    GuardianMultisig,
    /// Both emergency vote AND guardians required
    BothRequired,
}

/// Dead man's switch configuration
///
/// Auto-engages protections if DAO is inactive for extended period.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeadMansSwitchConfig {
    /// Inactivity threshold in blocks (no proposals/votes)
    pub inactivity_threshold_blocks: u64,
    /// Auto-engage rate limit (bps of TVL per epoch)
    pub auto_rate_limit_bps: u64,
    /// Notify members after notification period
    pub notification_blocks: u64,
    /// Enable social recovery mode after switch
    pub enable_social_recovery: bool,
    /// Social recovery timelock (blocks)
    pub social_recovery_timelock_blocks: u64,
}

impl Default for DeadMansSwitchConfig {
    fn default() -> Self {
        Self {
            // Trigger after 30 days of no activity
            inactivity_threshold_blocks: 30 * 24 * 6,
            // Auto-limit to 1% TVL per day
            auto_rate_limit_bps: 100,
            // Notify for 7 days before switch
            notification_blocks: 7 * 24 * 6,
            // Enable member claims in recovery
            enable_social_recovery: true,
            // 14 day timelock for recovery
            social_recovery_timelock_blocks: 14 * 24 * 6,
        }
    }
}

// ============================================================================
// EXIT QUEUE STATE
// ============================================================================

/// Entry in the exit queue
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExitQueueEntry {
    /// Queue position (FIFO order)
    pub position: u64,
    /// Member public key
    pub member_pubkey: PublicKey,
    /// Requested exit value
    pub requested_value: u64,
    /// Weight at time of request
    pub weight: u64,
    /// Block when queued
    pub queued_at: u64,
    /// Whether processed
    pub processed: bool,
}

// ============================================================================
// CIRCUIT BREAKER STATE
// ============================================================================

/// Circuit breaker state
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CircuitBreakerState {
    /// Whether currently paused
    pub paused: bool,
    /// Block when pause was triggered
    pub pause_triggered_at: u64,
    /// Block when auto-resume would occur
    pub auto_resume_at: u64,
    /// Amount drained in current window
    pub drained_in_window: u64,
    /// Guardian notified at block
    pub guardian_notified_at: u64,
}

// ============================================================================
// GUARDIAN PAUSE STATE — replaced by MultiSig composition
// ============================================================================
// Guardian pause/unpause now validates MultiSig::FinalizeV1 child calls.
// guardian_group_id references a MultiSig group created during init.
// ============================================================================

// ============================================================================
// DEAD MAN'S SWITCH STATE
// ============================================================================

/// Dead man's switch state
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeadMansSwitchState {
    /// Whether switch has been triggered
    pub triggered: bool,
    /// Last activity block (proposal or vote)
    pub last_activity_at: u64,
    /// Notification sent at block
    pub notification_sent_at: u64,
    /// Social recovery mode activated at block
    pub recovery_activated_at: u64,
}