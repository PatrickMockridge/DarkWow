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

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, PublicKey},
    error::ContractError,
    pasta::pallas,
};
use dwow_serial::{deserialize, serialize, SerialDecodable, SerialEncodable};

/// Unique identifier for the protected fund (derived from DAO-Escrow bulla)
pub type FundId = pallas::Base;

// ============================================================================
// STORED-ONLY SUB-TYPES (manual encode/decode, no SerialEncodable)
// ============================================================================

#[derive(Debug, Clone)]
pub struct MemberWeight {
    pub contribution: u64,
    pub deposited_at: u64,
    pub weight_multiplier: u64,
}

impl MemberWeight {
    pub fn effective_weight(&self, current_block: u64) -> u64 {
        let blocks_held = current_block.saturating_sub(self.deposited_at);
        let time_multiplier = 1_000 + (blocks_held / 10_000).min(2_000);
        self.contribution * time_multiplier / 1_000
    }
}

#[derive(Debug, Clone)]
pub struct ObservationPending {
    pub proposal_id: pallas::Base,
    pub amount: u64,
    pub observation_ends_at: u64,
}

#[derive(Debug, Clone)]
pub struct ExitQueueEntry {
    pub position: u64,
    pub member_pubkey: PublicKey,
    pub requested_value: u64,
    pub weight: u64,
    pub queued_at: u64,
    pub processed: bool,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerState {
    pub paused: bool,
    pub pause_triggered_at: u64,
    pub auto_resume_at: u64,
    pub drained_in_window: u64,
    pub guardian_notified_at: u64,
}

#[derive(Debug, Clone)]
pub struct DeadMansSwitchState {
    pub triggered: bool,
    pub last_activity_at: u64,
    pub notification_sent_at: u64,
    pub recovery_activated_at: u64,
}

#[derive(Debug, Clone)]
pub struct TransferRecord {
    pub version: u8,
    pub block: u64,
    pub amount: u64,
}

#[derive(Debug, Clone)]
pub struct ExitRequest {
    pub id: pallas::Base,
    pub member_pubkey: PublicKey,
    pub weight: u64,
    pub requested_value: u64,
    pub haircut_bps: u64,
    pub payout_value: u64,
    pub requested_at: u64,
    pub processed: bool,
}

// ============================================================================
// ENUMS AND CONFIG TYPES (keep SerialEncodable — used in params)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockState {
    Unlocked = 0,
    Locked = 1,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RateLimit {
    pub base_rate_bps: u64,
    pub averaging_window_blocks: u64,
    pub vote_required_above_bps: u64,
}

impl Default for RateLimit {
    fn default() -> Self {
        Self { base_rate_bps: 100, averaging_window_blocks: 1000, vote_required_above_bps: 100 }
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExitQueueConfig {
    pub max_exit_per_epoch_bps: u64,
    pub epoch_blocks: u64,
    pub min_queue_blocks: u64,
    pub force_fcfs: bool,
}

impl Default for ExitQueueConfig {
    fn default() -> Self {
        Self { max_exit_per_epoch_bps: 1000, epoch_blocks: 600, min_queue_blocks: 10, force_fcfs: true }
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CircuitBreakerConfig {
    pub trigger_threshold_bps: u64,
    pub window_blocks: u64,
    pub pause_duration_blocks: u64,
    pub auto_resume: bool,
    pub notify_guardians: bool,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self { trigger_threshold_bps: 1000, window_blocks: 100, pause_duration_blocks: 600, auto_resume: false, notify_guardians: true }
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ObservationPeriodConfig {
    pub threshold_bps: u64,
    pub observation_blocks: u64,
    pub allow_emergency_bypass: bool,
    pub emergency_bypass_quorum_bps: u64,
}

impl Default for ObservationPeriodConfig {
    fn default() -> Self {
        Self { threshold_bps: 500, observation_blocks: 48 * 6, allow_emergency_bypass: true, emergency_bypass_quorum_bps: 9000 }
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SplitProposalsConfig {
    pub threshold_bps: u64,
    pub max_chunk_bps: u64,
    pub chunk_delay_blocks: u64,
    pub separate_vote_each_chunk: bool,
}

impl Default for SplitProposalsConfig {
    fn default() -> Self {
        Self { threshold_bps: 1000, max_chunk_bps: 1000, chunk_delay_blocks: 600, separate_vote_each_chunk: true }
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum ReserveSpendAuthority {
    EmergencyVoteOnly,
    GuardianMultisig,
    BothRequired,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct NoLossReserveConfig {
    pub reserve_bps: u64,
    pub reserve_spend_authority: ReserveSpendAuthority,
    pub min_reserve_absolute: u64,
}

impl Default for NoLossReserveConfig {
    fn default() -> Self {
        Self { reserve_bps: 2000, reserve_spend_authority: ReserveSpendAuthority::EmergencyVoteOnly, min_reserve_absolute: 100 }
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeadMansSwitchConfig {
    pub inactivity_threshold_blocks: u64,
    pub auto_rate_limit_bps: u64,
    pub notification_blocks: u64,
    pub enable_social_recovery: bool,
    pub social_recovery_timelock_blocks: u64,
}

impl Default for DeadMansSwitchConfig {
    fn default() -> Self {
        Self { inactivity_threshold_blocks: 30 * 24 * 6, auto_rate_limit_bps: 100, notification_blocks: 7 * 24 * 6, enable_social_recovery: true, social_recovery_timelock_blocks: 14 * 24 * 6 }
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DrainConfig {
    pub guardian_multisig_group_id: pallas::Base,
    pub exit_queue: Option<ExitQueueConfig>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    pub observation_period: Option<ObservationPeriodConfig>,
    pub split_proposals: Option<SplitProposalsConfig>,
    pub no_loss_reserve: Option<NoLossReserveConfig>,
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

// ============================================================================
// PROTECTED FUND (main stored type — manual encode/decode)
// ============================================================================

#[derive(Debug, Clone)]
pub struct ProtectedFund {
    pub version: u8,
    pub instance_seed: [u8; 32],
    pub id: FundId,
    pub total_funds: u64,
    pub spend_authority: PublicKey,
    pub lock_state: LockState,
    pub rate_limit: RateLimit,
    pub multisig_group_id: pallas::Base,
    pub purse_id: pallas::Base,
    pub drain_config: DrainConfig,
    pub members: Vec<MemberWeight>,
    pub lock_expires_at: u64,
    pub authority_change_timelock: u64,
    pub created_at: u64,
    pub exit_queue_state: Vec<ExitQueueEntry>,
    pub circuit_breaker_state: Option<CircuitBreakerState>,
    pub dead_mans_switch_state: Option<DeadMansSwitchState>,
    pub no_loss_reserve_balance: u64,
    pub observation_pending: Vec<ObservationPending>,
}

// ============================================================================
// PARAMS (keep SerialEncodable/SerialDecodable)
// ============================================================================

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParamsV1 {
    pub instance_seed: [u8; 32],
    pub fund_id: FundId,
    pub spend_authority: PublicKey,
    pub dao_escrow_bulla: pallas::Base,
    pub drain_config: DrainConfig,
}

#[derive(Debug, Clone)]
pub struct InitializeUpdateV1 {
    pub instance_seed: [u8; 32],
    pub fund_id: FundId,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProposeParamsV1 {
    pub message_hash: pallas::Base,
    pub multisig_group_id: pallas::Base,
    pub prover_pubkey: PublicKey,
    pub vote_period_blocks: u64,
    pub proof: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ProposeUpdateV1 {
    pub proposal_id: pallas::Base,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VoteParamsV1 {
    pub proposal_id: pallas::Base,
    pub voter_pubkey: PublicKey,
    pub vote: bool,
    pub signature: pallas::Base,
}

#[derive(Debug, Clone)]
pub struct VoteUpdateV1 {
    pub proposal_id: pallas::Base,
    pub yes_votes: u64,
    pub no_votes: u64,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteParamsV1 {
    pub proposal_id: pallas::Base,
    pub signature: pallas::Base,
}

#[derive(Debug, Clone)]
pub struct ExecuteUpdateV1 {
    pub proposal_id: pallas::Base,
    pub action: pallas::Base,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExitParamsV1 {
    pub fund_id: FundId,
    pub member_pubkey: PublicKey,
    pub contribution_weight: u64,
    pub current_block: u64,
    pub dao_escrow_bulla: pallas::Base,
    pub dao_membership_note: pallas::Base,
    pub effective_weight: pallas::Base,
    pub proof: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ExitUpdateV1 {
    pub exit_id: pallas::Base,
    pub member_pubkey: PublicKey,
    pub payout_value: u64,
    pub haircut_collected: u64,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferParamsV1 {
    pub fund_id: FundId,
    pub amount: u64,
    pub recipient: PublicKey,
    pub signature: pallas::Base,
    pub exceeds_rate_limit: bool,
    pub vote_proposal_id: Option<pallas::Base>,
}

#[derive(Debug, Clone)]
pub struct TransferUpdateV1 {
    pub amount: u64,
    pub recipient: PublicKey,
    pub rate_limited: bool,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LockParamsV1 {
    pub fund_id: FundId,
    pub duration_blocks: u64,
    pub signature: pallas::Base,
}

#[derive(Debug, Clone)]
pub struct LockUpdateV1 {
    pub locked_until: u64,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnlockParamsV1 {
    pub fund_id: FundId,
    pub signature: pallas::Base,
}

#[derive(Debug, Clone)]
pub struct UnlockUpdateV1 {
    pub unlocked_at: u64,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigParamsV1 {
    pub fund_id: FundId,
    pub rate_limit: Option<RateLimit>,
    pub multisig_group_id: Option<pallas::Base>,
    pub new_spend_authority: Option<PublicKey>,
}

#[derive(Debug, Clone)]
pub struct UpdateConfigUpdateV1 {
    pub authority_change_timelock: Option<u64>,
}

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
// ============================================================================

// --- Stored-only sub-types ---

impl MemberWeight {
    pub const ENCODED_SIZE: usize = 24;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.contribution.to_le_bytes());
        buf.extend_from_slice(&self.deposited_at.to_le_bytes());
        buf.extend_from_slice(&self.weight_multiplier.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("MemberWeight: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(MemberWeight {
            contribution: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            deposited_at: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            weight_multiplier: u64::from_le_bytes(data[16..24].try_into().unwrap()),
        })
    }
}

impl ObservationPending {
    pub const ENCODED_SIZE: usize = 48;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.proposal_id.to_repr());
        buf.extend_from_slice(&self.amount.to_le_bytes());
        buf.extend_from_slice(&self.observation_ends_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("ObservationPending: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(ObservationPending {
            proposal_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("ObservationPending: invalid proposal_id".into()))?,
            amount: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            observation_ends_at: u64::from_le_bytes(data[40..48].try_into().unwrap()),
        })
    }
}

impl ExitQueueEntry {
    pub const ENCODED_SIZE: usize = 65;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.position.to_le_bytes());
        buf.extend_from_slice(&self.member_pubkey.to_bytes());
        buf.extend_from_slice(&self.requested_value.to_le_bytes());
        buf.extend_from_slice(&self.weight.to_le_bytes());
        buf.extend_from_slice(&self.queued_at.to_le_bytes());
        buf.push(if self.processed { 1 } else { 0 });
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("ExitQueueEntry: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(ExitQueueEntry {
            position: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            member_pubkey: PublicKey::from_bytes(data[8..40].try_into().unwrap())
                .map_err(|e| ContractError::IoError(format!("ExitQueueEntry: invalid member_pubkey: {:?}", e)))?,
            requested_value: u64::from_le_bytes(data[40..48].try_into().unwrap()),
            weight: u64::from_le_bytes(data[48..56].try_into().unwrap()),
            queued_at: u64::from_le_bytes(data[56..64].try_into().unwrap()),
            processed: data[64] != 0,
        })
    }
}

impl CircuitBreakerState {
    pub const ENCODED_SIZE: usize = 33;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(if self.paused { 1 } else { 0 });
        buf.extend_from_slice(&self.pause_triggered_at.to_le_bytes());
        buf.extend_from_slice(&self.auto_resume_at.to_le_bytes());
        buf.extend_from_slice(&self.drained_in_window.to_le_bytes());
        buf.extend_from_slice(&self.guardian_notified_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("CircuitBreakerState: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(CircuitBreakerState {
            paused: data[0] != 0,
            pause_triggered_at: u64::from_le_bytes(data[1..9].try_into().unwrap()),
            auto_resume_at: u64::from_le_bytes(data[9..17].try_into().unwrap()),
            drained_in_window: u64::from_le_bytes(data[17..25].try_into().unwrap()),
            guardian_notified_at: u64::from_le_bytes(data[25..33].try_into().unwrap()),
        })
    }
}

impl DeadMansSwitchState {
    pub const ENCODED_SIZE: usize = 25;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(if self.triggered { 1 } else { 0 });
        buf.extend_from_slice(&self.last_activity_at.to_le_bytes());
        buf.extend_from_slice(&self.notification_sent_at.to_le_bytes());
        buf.extend_from_slice(&self.recovery_activated_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("DeadMansSwitchState: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(DeadMansSwitchState {
            triggered: data[0] != 0,
            last_activity_at: u64::from_le_bytes(data[1..9].try_into().unwrap()),
            notification_sent_at: u64::from_le_bytes(data[9..17].try_into().unwrap()),
            recovery_activated_at: u64::from_le_bytes(data[17..25].try_into().unwrap()),
        })
    }
}

impl TransferRecord {
    pub const ENCODED_SIZE: usize = 17;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.block.to_le_bytes());
        buf.extend_from_slice(&self.amount.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("TransferRecord: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(TransferRecord { version: data[0], block: u64::from_le_bytes(data[1..9].try_into().unwrap()), amount: u64::from_le_bytes(data[9..17].try_into().unwrap()) })
    }
}

// --- ProtectedFund (main stored type) ---

impl ProtectedFund {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(self.version);
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.id.to_repr());
        buf.extend_from_slice(&self.total_funds.to_le_bytes());
        buf.extend_from_slice(&self.spend_authority.to_bytes());
        buf.push(self.lock_state as u8);
        // rate_limit: use dwow_serial (has SerialEncodable)
        let rl_enc = serialize(&self.rate_limit);
        buf.extend_from_slice(&(rl_enc.len() as u32).to_le_bytes());
        buf.extend_from_slice(&rl_enc);
        buf.extend_from_slice(&self.multisig_group_id.to_repr());
        buf.extend_from_slice(&self.purse_id.to_repr());
        // drain_config: use dwow_serial (has SerialEncodable)
        let dc_enc = serialize(&self.drain_config);
        buf.extend_from_slice(&(dc_enc.len() as u32).to_le_bytes());
        buf.extend_from_slice(&dc_enc);
        // members: Vec<MemberWeight>
        buf.push(self.members.len() as u8);
        for m in &self.members { buf.extend_from_slice(&m.encode()); }
        buf.extend_from_slice(&self.lock_expires_at.to_le_bytes());
        buf.extend_from_slice(&self.authority_change_timelock.to_le_bytes());
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        // exit_queue_state
        buf.push(self.exit_queue_state.len() as u8);
        for e in &self.exit_queue_state { buf.extend_from_slice(&e.encode()); }
        // circuit_breaker_state
        match &self.circuit_breaker_state {
            Some(s) => { buf.push(1); buf.extend_from_slice(&s.encode()); }
            None => { buf.push(0); }
        }
        // dead_mans_switch_state
        match &self.dead_mans_switch_state {
            Some(s) => { buf.push(1); buf.extend_from_slice(&s.encode()); }
            None => { buf.push(0); }
        }
        buf.extend_from_slice(&self.no_loss_reserve_balance.to_le_bytes());
        // observation_pending
        buf.push(self.observation_pending.len() as u8);
        for o in &self.observation_pending { buf.extend_from_slice(&o.encode()); }
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let mut pos: usize = 0;
        if data.len() < 194 { return Err(ContractError::IoError("ProtectedFund: data too short".into())); }
        let version = data[pos]; pos += 1;
        let instance_seed: [u8; 32] = data[pos..pos+32].try_into().unwrap(); pos += 32;
        let id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ProtectedFund: invalid id".into()))?; pos += 32;
        let total_funds = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let spend_authority = PublicKey::from_bytes(data[pos..pos+32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("ProtectedFund: invalid spend_authority: {:?}", e)))?; pos += 32;
        let lock_state = match data[pos] { 0 => LockState::Unlocked, 1 => LockState::Locked, _ => return Err(ContractError::IoError("ProtectedFund: invalid lock_state".into())) }; pos += 1;
        // rate_limit: deserialize length-prefixed
        if pos + 4 > data.len() { return Err(ContractError::IoError("ProtectedFund: data too short for rate_limit len".into())); }
        let rl_len = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()) as usize; pos += 4;
        if pos + rl_len > data.len() { return Err(ContractError::IoError("ProtectedFund: data too short for rate_limit".into())); }
        let rate_limit: RateLimit = deserialize(&data[pos..pos+rl_len])?; pos += rl_len;
        let multisig_group_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ProtectedFund: invalid multisig_group_id".into()))?; pos += 32;
        let purse_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ProtectedFund: invalid purse_id".into()))?; pos += 32;
        // drain_config: deserialize length-prefixed
        if pos + 4 > data.len() { return Err(ContractError::IoError("ProtectedFund: data too short for drain_config len".into())); }
        let dc_len = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()) as usize; pos += 4;
        if pos + dc_len > data.len() { return Err(ContractError::IoError("ProtectedFund: data too short for drain_config".into())); }
        let drain_config: DrainConfig = deserialize(&data[pos..pos+dc_len])?; pos += dc_len;
        // members
        if pos >= data.len() { return Err(ContractError::IoError("ProtectedFund: data too short for members count".into())); }
        let member_count = data[pos] as usize; pos += 1;
        let mut members = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            members.push(MemberWeight::decode(&data[pos..pos+MemberWeight::ENCODED_SIZE])?);
            pos += MemberWeight::ENCODED_SIZE;
        }
        if pos + 24 > data.len() { return Err(ContractError::IoError("ProtectedFund: data too short for tail".into())); }
        let lock_expires_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let authority_change_timelock = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let created_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        // exit_queue_state
        if pos >= data.len() { return Err(ContractError::IoError("ProtectedFund: data too short for exit_queue_state count".into())); }
        let eq_count = data[pos] as usize; pos += 1;
        let mut exit_queue_state = Vec::with_capacity(eq_count);
        for _ in 0..eq_count {
            exit_queue_state.push(ExitQueueEntry::decode(&data[pos..pos+ExitQueueEntry::ENCODED_SIZE])?);
            pos += ExitQueueEntry::ENCODED_SIZE;
        }
        // circuit_breaker_state
        if pos >= data.len() { return Err(ContractError::IoError("ProtectedFund: data too short for cb flag".into())); }
        let circuit_breaker_state = if data[pos] == 1 {
            pos += 1;
            let s = CircuitBreakerState::decode(&data[pos..pos+CircuitBreakerState::ENCODED_SIZE])?;
            pos += CircuitBreakerState::ENCODED_SIZE;
            Some(s)
        } else { pos += 1; None };
        // dead_mans_switch_state
        if pos >= data.len() { return Err(ContractError::IoError("ProtectedFund: data too short for dms flag".into())); }
        let dead_mans_switch_state = if data[pos] == 1 {
            pos += 1;
            let s = DeadMansSwitchState::decode(&data[pos..pos+DeadMansSwitchState::ENCODED_SIZE])?;
            pos += DeadMansSwitchState::ENCODED_SIZE;
            Some(s)
        } else { pos += 1; None };
        if pos + 8 > data.len() { return Err(ContractError::IoError("ProtectedFund: data too short for no_loss_reserve_balance".into())); }
        let no_loss_reserve_balance = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        // observation_pending
        if pos >= data.len() { return Err(ContractError::IoError("ProtectedFund: data too short for obs count".into())); }
        let obs_count = data[pos] as usize; pos += 1;
        let mut observation_pending = Vec::with_capacity(obs_count);
        for _ in 0..obs_count {
            observation_pending.push(ObservationPending::decode(&data[pos..pos+ObservationPending::ENCODED_SIZE])?);
            pos += ObservationPending::ENCODED_SIZE;
        }
        Ok(ProtectedFund {
            version, instance_seed, id, total_funds, spend_authority, lock_state,
            rate_limit, multisig_group_id, purse_id, drain_config, members,
            lock_expires_at, authority_change_timelock, created_at, exit_queue_state,
            circuit_breaker_state, dead_mans_switch_state, no_loss_reserve_balance,
            observation_pending,
        })
    }
}

// --- Bridge update structs ---

impl InitializeUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.fund_id.to_repr());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("InitializeUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(InitializeUpdateV1 {
            instance_seed: data[0..32].try_into().unwrap(),
            fund_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("InitializeUpdateV1: invalid fund_id".into()))?,
        })
    }
}

impl ProposeUpdateV1 {
    pub const ENCODED_SIZE: usize = 32;
    pub fn encode(&self) -> Vec<u8> { self.proposal_id.to_repr().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("ProposeUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(ProposeUpdateV1 {
            proposal_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("ProposeUpdateV1: invalid proposal_id".into()))?,
        })
    }
}

impl VoteUpdateV1 {
    pub const ENCODED_SIZE: usize = 48;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.proposal_id.to_repr());
        buf.extend_from_slice(&self.yes_votes.to_le_bytes());
        buf.extend_from_slice(&self.no_votes.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("VoteUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(VoteUpdateV1 {
            proposal_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("VoteUpdateV1: invalid proposal_id".into()))?,
            yes_votes: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            no_votes: u64::from_le_bytes(data[40..48].try_into().unwrap()),
        })
    }
}

impl ExecuteUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.proposal_id.to_repr());
        buf.extend_from_slice(&self.action.to_repr());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("ExecuteUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(ExecuteUpdateV1 {
            proposal_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("ExecuteUpdateV1: invalid proposal_id".into()))?,
            action: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("ExecuteUpdateV1: invalid action".into()))?,
        })
    }
}

impl ExitUpdateV1 {
    pub const ENCODED_SIZE: usize = 80;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.exit_id.to_repr());
        buf.extend_from_slice(&self.member_pubkey.to_bytes());
        buf.extend_from_slice(&self.payout_value.to_le_bytes());
        buf.extend_from_slice(&self.haircut_collected.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("ExitUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(ExitUpdateV1 {
            exit_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("ExitUpdateV1: invalid exit_id".into()))?,
            member_pubkey: PublicKey::from_bytes(data[32..64].try_into().unwrap())
                .map_err(|e| ContractError::IoError(format!("ExitUpdateV1: invalid member_pubkey: {:?}", e)))?,
            payout_value: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            haircut_collected: u64::from_le_bytes(data[72..80].try_into().unwrap()),
        })
    }
}

impl TransferUpdateV1 {
    pub const ENCODED_SIZE: usize = 41;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.amount.to_le_bytes());
        buf.extend_from_slice(&self.recipient.to_bytes());
        buf.push(if self.rate_limited { 1 } else { 0 });
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("TransferUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(TransferUpdateV1 {
            amount: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            recipient: PublicKey::from_bytes(data[8..40].try_into().unwrap())
                .map_err(|e| ContractError::IoError(format!("TransferUpdateV1: invalid recipient: {:?}", e)))?,
            rate_limited: data[40] != 0,
        })
    }
}

impl LockUpdateV1 {
    pub const ENCODED_SIZE: usize = 8;
    pub fn encode(&self) -> Vec<u8> { self.locked_until.to_le_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("LockUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(LockUpdateV1 { locked_until: u64::from_le_bytes(data[0..8].try_into().unwrap()) })
    }
}

impl UnlockUpdateV1 {
    pub const ENCODED_SIZE: usize = 8;
    pub fn encode(&self) -> Vec<u8> { self.unlocked_at.to_le_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("UnlockUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(UnlockUpdateV1 { unlocked_at: u64::from_le_bytes(data[0..8].try_into().unwrap()) })
    }
}

impl UpdateConfigUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self.authority_change_timelock {
            Some(v) => { buf.push(1); buf.extend_from_slice(&v.to_le_bytes()); }
            None => { buf.push(0); }
        }
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("UpdateConfigUpdateV1: empty data".into())); }
        let authority_change_timelock = if data[0] == 1 {
            if data.len() < 9 { return Err(ContractError::IoError("UpdateConfigUpdateV1: data too short".into())); }
            Some(u64::from_le_bytes(data[1..9].try_into().unwrap()))
        } else { None };
        Ok(UpdateConfigUpdateV1 { authority_change_timelock })
    }
}
