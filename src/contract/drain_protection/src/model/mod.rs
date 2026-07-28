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

#[derive(Debug, Clone,)]
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

impl RateLimit { pub const ENCODED_SIZE: usize = 24; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(24); b.extend_from_slice(&self.base_rate_bps.to_le_bytes()); b.extend_from_slice(&self.averaging_window_blocks.to_le_bytes()); b.extend_from_slice(&self.vote_required_above_bps.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 24 { return Err(ContractError::IoError(format!("RateLimit: expected 24 bytes, got {}", data.len()))); } Ok(RateLimit { base_rate_bps: u64::from_le_bytes(data[0..8].try_into().unwrap()), averaging_window_blocks: u64::from_le_bytes(data[8..16].try_into().unwrap()), vote_required_above_bps: u64::from_le_bytes(data[16..24].try_into().unwrap()) }) } }

#[derive(Debug, Clone,)]
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
impl ExitQueueConfig { pub const ENCODED_SIZE: usize = 25; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(25); b.extend_from_slice(&self.max_exit_per_epoch_bps.to_le_bytes()); b.extend_from_slice(&self.epoch_blocks.to_le_bytes()); b.extend_from_slice(&self.min_queue_blocks.to_le_bytes()); b.push(self.force_fcfs as u8); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 25 { return Err(ContractError::IoError(format!("ExitQueueConfig: expected 25 bytes, got {}", data.len()))); } Ok(ExitQueueConfig { max_exit_per_epoch_bps: u64::from_le_bytes(data[0..8].try_into().unwrap()), epoch_blocks: u64::from_le_bytes(data[8..16].try_into().unwrap()), min_queue_blocks: u64::from_le_bytes(data[16..24].try_into().unwrap()), force_fcfs: data[24] != 0 }) } }

#[derive(Debug, Clone,)] pub struct CircuitBreakerConfig { pub trigger_threshold_bps: u64, pub window_blocks: u64, pub pause_duration_blocks: u64, pub auto_resume: bool, pub notify_guardians: bool }
impl Default for CircuitBreakerConfig { fn default() -> Self { Self { trigger_threshold_bps: 1000, window_blocks: 100, pause_duration_blocks: 600, auto_resume: false, notify_guardians: true } } }
impl CircuitBreakerConfig { pub const ENCODED_SIZE: usize = 26; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(26); b.extend_from_slice(&self.trigger_threshold_bps.to_le_bytes()); b.extend_from_slice(&self.window_blocks.to_le_bytes()); b.extend_from_slice(&self.pause_duration_blocks.to_le_bytes()); b.push(self.auto_resume as u8); b.push(self.notify_guardians as u8); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 26 { return Err(ContractError::IoError(format!("CircuitBreakerConfig: expected 26 bytes, got {}", data.len()))); } Ok(CircuitBreakerConfig { trigger_threshold_bps: u64::from_le_bytes(data[0..8].try_into().unwrap()), window_blocks: u64::from_le_bytes(data[8..16].try_into().unwrap()), pause_duration_blocks: u64::from_le_bytes(data[16..24].try_into().unwrap()), auto_resume: data[24] != 0, notify_guardians: data[25] != 0 }) } }

#[derive(Debug, Clone,)] pub struct ObservationPeriodConfig { pub threshold_bps: u64, pub observation_blocks: u64, pub allow_emergency_bypass: bool, pub emergency_bypass_quorum_bps: u64 }
impl Default for ObservationPeriodConfig { fn default() -> Self { Self { threshold_bps: 500, observation_blocks: 48 * 6, allow_emergency_bypass: true, emergency_bypass_quorum_bps: 9000 } } }
impl ObservationPeriodConfig { pub const ENCODED_SIZE: usize = 25; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(25); b.extend_from_slice(&self.threshold_bps.to_le_bytes()); b.extend_from_slice(&self.observation_blocks.to_le_bytes()); b.push(self.allow_emergency_bypass as u8); b.extend_from_slice(&self.emergency_bypass_quorum_bps.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 25 { return Err(ContractError::IoError(format!("ObservationPeriodConfig: expected 25 bytes, got {}", data.len()))); } Ok(ObservationPeriodConfig { threshold_bps: u64::from_le_bytes(data[0..8].try_into().unwrap()), observation_blocks: u64::from_le_bytes(data[8..16].try_into().unwrap()), allow_emergency_bypass: data[16] != 0, emergency_bypass_quorum_bps: u64::from_le_bytes(data[17..25].try_into().unwrap()) }) } }

#[derive(Debug, Clone,)] pub struct SplitProposalsConfig { pub threshold_bps: u64, pub max_chunk_bps: u64, pub chunk_delay_blocks: u64, pub separate_vote_each_chunk: bool }
impl Default for SplitProposalsConfig { fn default() -> Self { Self { threshold_bps: 1000, max_chunk_bps: 1000, chunk_delay_blocks: 600, separate_vote_each_chunk: true } } }
impl SplitProposalsConfig { pub const ENCODED_SIZE: usize = 25; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(25); b.extend_from_slice(&self.threshold_bps.to_le_bytes()); b.extend_from_slice(&self.max_chunk_bps.to_le_bytes()); b.extend_from_slice(&self.chunk_delay_blocks.to_le_bytes()); b.push(self.separate_vote_each_chunk as u8); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 25 { return Err(ContractError::IoError(format!("SplitProposalsConfig: expected 25 bytes, got {}", data.len()))); } Ok(SplitProposalsConfig { threshold_bps: u64::from_le_bytes(data[0..8].try_into().unwrap()), max_chunk_bps: u64::from_le_bytes(data[8..16].try_into().unwrap()), chunk_delay_blocks: u64::from_le_bytes(data[16..24].try_into().unwrap()), separate_vote_each_chunk: data[24] != 0 }) } }

#[derive(Debug, Clone,)] pub enum ReserveSpendAuthority { EmergencyVoteOnly, GuardianMultisig, BothRequired }
impl TryFrom<u8> for ReserveSpendAuthority { type Error = ContractError; fn try_from(v: u8) -> Result<Self, Self::Error> { match v { 0 => Ok(Self::EmergencyVoteOnly), 1 => Ok(Self::GuardianMultisig), 2 => Ok(Self::BothRequired), _ => Err(ContractError::InvalidFunction) } } }
impl ReserveSpendAuthority { pub fn encode(&self) -> Vec<u8> { vec![self.clone() as u8] } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.is_empty() { return Err(ContractError::IoError("ReserveSpendAuthority: empty".into())); } Self::try_from(data[0]) } }

#[derive(Debug, Clone,)] pub struct NoLossReserveConfig { pub reserve_bps: u64, pub reserve_spend_authority: ReserveSpendAuthority, pub min_reserve_absolute: u64 }
impl Default for NoLossReserveConfig { fn default() -> Self { Self { reserve_bps: 2000, reserve_spend_authority: ReserveSpendAuthority::EmergencyVoteOnly, min_reserve_absolute: 100 } } }
impl NoLossReserveConfig { pub const ENCODED_SIZE: usize = 17; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(17); b.extend_from_slice(&self.reserve_bps.to_le_bytes()); b.extend_from_slice(&self.reserve_spend_authority.encode()); b.extend_from_slice(&self.min_reserve_absolute.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 17 { return Err(ContractError::IoError(format!("NoLossReserveConfig: expected 17 bytes, got {}", data.len()))); } Ok(NoLossReserveConfig { reserve_bps: u64::from_le_bytes(data[0..8].try_into().unwrap()), reserve_spend_authority: ReserveSpendAuthority::decode(&data[8..9])?, min_reserve_absolute: u64::from_le_bytes(data[9..17].try_into().unwrap()) }) } }

#[derive(Debug, Clone,)]
pub struct DeadMansSwitchConfig {
    pub inactivity_threshold_blocks: u64,
    pub auto_rate_limit_bps: u64,
    pub notification_blocks: u64,
    pub enable_social_recovery: bool,
    pub social_recovery_timelock_blocks: u64,
}

impl DeadMansSwitchConfig { pub const ENCODED_SIZE: usize = 33; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(33); b.extend_from_slice(&self.inactivity_threshold_blocks.to_le_bytes()); b.extend_from_slice(&self.auto_rate_limit_bps.to_le_bytes()); b.extend_from_slice(&self.notification_blocks.to_le_bytes()); b.push(self.enable_social_recovery as u8); b.extend_from_slice(&self.social_recovery_timelock_blocks.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 33 { return Err(ContractError::IoError(format!("DeadMansSwitchConfig: expected 33 bytes, got {}", data.len()))); } Ok(DeadMansSwitchConfig { inactivity_threshold_blocks: u64::from_le_bytes(data[0..8].try_into().unwrap()), auto_rate_limit_bps: u64::from_le_bytes(data[8..16].try_into().unwrap()), notification_blocks: u64::from_le_bytes(data[16..24].try_into().unwrap()), enable_social_recovery: data[24] != 0, social_recovery_timelock_blocks: u64::from_le_bytes(data[25..33].try_into().unwrap()) }) } }

impl Default for DeadMansSwitchConfig {
    fn default() -> Self {
        Self { inactivity_threshold_blocks: 30 * 24 * 6, auto_rate_limit_bps: 100, notification_blocks: 7 * 24 * 6, enable_social_recovery: true, social_recovery_timelock_blocks: 14 * 24 * 6 }
    }
}

#[derive(Debug, Clone,)]
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

impl DrainConfig { pub fn encode(&self) -> Vec<u8> { let eq = if let Some(ref v) = self.exit_queue { v.encode() } else { vec![] }; let cb = if let Some(ref v) = self.circuit_breaker { v.encode() } else { vec![] }; let op = if let Some(ref v) = self.observation_period { v.encode() } else { vec![] }; let sp = if let Some(ref v) = self.split_proposals { v.encode() } else { vec![] }; let nl = if let Some(ref v) = self.no_loss_reserve { v.encode() } else { vec![] }; let dm = if let Some(ref v) = self.dead_mans_switch { v.encode() } else { vec![] }; let mut b = Vec::with_capacity(38+eq.len()+cb.len()+op.len()+sp.len()+nl.len()+dm.len()); b.extend_from_slice(&self.guardian_multisig_group_id.to_repr()); b.push(self.exit_queue.is_some() as u8); b.extend_from_slice(&eq); b.push(self.circuit_breaker.is_some() as u8); b.extend_from_slice(&cb); b.push(self.observation_period.is_some() as u8); b.extend_from_slice(&op); b.push(self.split_proposals.is_some() as u8); b.extend_from_slice(&sp); b.push(self.no_loss_reserve.is_some() as u8); b.extend_from_slice(&nl); b.push(self.dead_mans_switch.is_some() as u8); b.extend_from_slice(&dm); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 38 { return Err(ContractError::IoError("DrainConfig: too short".into())); } let guardian_multisig_group_id = read_base(&data[0..32])?; let mut pos = 32; let has_eq = data[pos] != 0; pos += 1; let exit_queue = if has_eq { let v = ExitQueueConfig::decode(&data[pos..])?; pos += v.encode().len(); Some(v) } else { None }; let has_cb = data[pos] != 0; pos += 1; let circuit_breaker = if has_cb { let v = CircuitBreakerConfig::decode(&data[pos..])?; pos += v.encode().len(); Some(v) } else { None }; let has_op = data[pos] != 0; pos += 1; let observation_period = if has_op { let v = ObservationPeriodConfig::decode(&data[pos..])?; pos += v.encode().len(); Some(v) } else { None }; let has_sp = data[pos] != 0; pos += 1; let split_proposals = if has_sp { let v = SplitProposalsConfig::decode(&data[pos..])?; pos += v.encode().len(); Some(v) } else { None }; let has_nl = data[pos] != 0; pos += 1; let no_loss_reserve = if has_nl { let v = NoLossReserveConfig::decode(&data[pos..])?; pos += v.encode().len(); Some(v) } else { None }; let has_dm = data[pos] != 0; pos += 1; let dead_mans_switch = if has_dm { let v = DeadMansSwitchConfig::decode(&data[pos..])?; Some(v) } else { None }; Ok(DrainConfig { guardian_multisig_group_id, exit_queue, circuit_breaker, observation_period, split_proposals, no_loss_reserve, dead_mans_switch }) } }

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
// PARAMS
// ============================================================================

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap())).ok_or_else(|| ContractError::IoError("invalid base".into())) }

#[derive(Debug, Clone,)]
pub struct InitializeParamsV1 {
    pub instance_seed: [u8; 32],
    pub fund_id: FundId,
    pub spend_authority: PublicKey,
    pub dao_escrow_bulla: pallas::Base,
    pub drain_config: DrainConfig,
}

impl InitializeParamsV1 { pub fn encode(&self) -> Vec<u8> { let dc = self.drain_config.encode(); let mut b = Vec::with_capacity(97+dc.len()); b.extend_from_slice(&self.instance_seed); b.extend_from_slice(&self.fund_id.to_repr()); b.extend_from_slice(&self.spend_authority.to_bytes()); b.extend_from_slice(&self.dao_escrow_bulla.to_repr()); b.extend_from_slice(&dc); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 97 { return Err(ContractError::IoError("InitializeParamsV1: too short".into())); } let instance_seed: [u8;32] = data[0..32].try_into().unwrap(); let fund_id = read_base(&data[32..64])?; let spend_authority = PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("InitializeParamsV1: invalid spend_authority: {}", e)))?; let dao_escrow_bulla = read_base(&data[96..128])?; let drain_config = DrainConfig::decode(&data[128..])?; Ok(InitializeParamsV1 { instance_seed, fund_id, spend_authority, dao_escrow_bulla, drain_config }) } }

#[derive(Debug, Clone)]
pub struct InitializeUpdateV1 {
    pub instance_seed: [u8; 32],
    pub fund_id: FundId,
}

#[derive(Debug, Clone,)]
pub struct ProposeParamsV1 {
    pub message_hash: pallas::Base,
    pub multisig_group_id: pallas::Base,
    pub prover_pubkey: PublicKey,
    pub vote_period_blocks: u64,
    pub proof: Vec<u8>,
}

impl ProposeParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(97+self.proof.len()); b.extend_from_slice(&self.message_hash.to_repr()); b.extend_from_slice(&self.multisig_group_id.to_repr()); b.extend_from_slice(&self.prover_pubkey.to_bytes()); b.extend_from_slice(&self.vote_period_blocks.to_le_bytes()); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 97 { return Err(ContractError::IoError("ProposeParamsV1: too short".into())); } let message_hash = read_base(&data[0..32])?; let multisig_group_id = read_base(&data[32..64])?; let prover_pubkey = PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ProposeParamsV1: invalid prover_pubkey: {}", e)))?; let vote_period_blocks = u64::from_le_bytes(data[96..104].try_into().unwrap()); let proof_len = data[104] as usize; if data.len() != 105+proof_len { return Err(ContractError::IoError(format!("ProposeParamsV1: expected {} bytes, got {}", 105+proof_len, data.len()))); } let proof = data[105..].to_vec(); Ok(ProposeParamsV1 { message_hash, multisig_group_id, prover_pubkey, vote_period_blocks, proof }) } }

#[derive(Debug, Clone)] pub struct ProposeUpdateV1 { pub proposal_id: pallas::Base }

#[derive(Debug, Clone,)] pub struct VoteParamsV1 { pub proposal_id: pallas::Base, pub voter_pubkey: PublicKey, pub vote: bool, pub signature: pallas::Base }
impl VoteParamsV1 { pub const ENCODED_SIZE: usize = 97; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(97); b.extend_from_slice(&self.proposal_id.to_repr()); b.extend_from_slice(&self.voter_pubkey.to_bytes()); b.push(self.vote as u8); b.extend_from_slice(&self.signature.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 97 { return Err(ContractError::IoError(format!("VoteParamsV1: expected 97 bytes, got {}", data.len()))); } Ok(VoteParamsV1 { proposal_id: read_base(&data[0..32])?, voter_pubkey: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("VoteParamsV1: invalid voter_pubkey: {}", e)))?, vote: data[64] != 0, signature: read_base(&data[65..97])? }) } }

#[derive(Debug, Clone)] pub struct VoteUpdateV1 { pub proposal_id: pallas::Base, pub yes_votes: u64, pub no_votes: u64 }

#[derive(Debug, Clone,)] pub struct ExecuteParamsV1 { pub proposal_id: pallas::Base, pub signature: pallas::Base }
impl ExecuteParamsV1 { pub const ENCODED_SIZE: usize = 64; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.proposal_id.to_repr()); b.extend_from_slice(&self.signature.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("ExecuteParamsV1: expected 64 bytes, got {}", data.len()))); } Ok(ExecuteParamsV1 { proposal_id: read_base(&data[0..32])?, signature: read_base(&data[32..64])? }) } }

#[derive(Debug, Clone)]
pub struct ExecuteUpdateV1 {
    pub proposal_id: pallas::Base,
    pub action: pallas::Base,
}

#[derive(Debug, Clone,)]
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

impl ExitParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(161+self.proof.len()); b.extend_from_slice(&self.fund_id.to_repr()); b.extend_from_slice(&self.member_pubkey.to_bytes()); b.extend_from_slice(&self.contribution_weight.to_le_bytes()); b.extend_from_slice(&self.current_block.to_le_bytes()); b.extend_from_slice(&self.dao_escrow_bulla.to_repr()); b.extend_from_slice(&self.dao_membership_note.to_repr()); b.extend_from_slice(&self.effective_weight.to_repr()); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 161 { return Err(ContractError::IoError("ExitParamsV1: too short".into())); } let fund_id = read_base(&data[0..32])?; let member_pubkey = PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ExitParamsV1: invalid member_pubkey: {}", e)))?; let contribution_weight = u64::from_le_bytes(data[64..72].try_into().unwrap()); let current_block = u64::from_le_bytes(data[72..80].try_into().unwrap()); let dao_escrow_bulla = read_base(&data[80..112])?; let dao_membership_note = read_base(&data[112..144])?; let effective_weight = read_base(&data[144..176])?; let proof_len = data[176] as usize; if data.len() != 177+proof_len { return Err(ContractError::IoError(format!("ExitParamsV1: expected {} bytes, got {}", 177+proof_len, data.len()))); } let proof = data[177..].to_vec(); Ok(ExitParamsV1 { fund_id, member_pubkey, contribution_weight, current_block, dao_escrow_bulla, dao_membership_note, effective_weight, proof }) } }

#[derive(Debug, Clone)] pub struct ExitUpdateV1 { pub exit_id: pallas::Base, pub member_pubkey: PublicKey, pub payout_value: u64, pub haircut_collected: u64 }

#[derive(Debug, Clone,)] pub struct TransferParamsV1 { pub fund_id: FundId, pub amount: u64, pub recipient: PublicKey, pub signature: pallas::Base, pub exceeds_rate_limit: bool, pub vote_proposal_id: Option<pallas::Base> }
impl TransferParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(106); b.extend_from_slice(&self.fund_id.to_repr()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.recipient.to_bytes()); b.extend_from_slice(&self.signature.to_repr()); b.push(self.exceeds_rate_limit as u8); b.push(self.vote_proposal_id.is_some() as u8); if let Some(v) = self.vote_proposal_id { b.extend_from_slice(&v.to_repr()); } b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 106 { return Err(ContractError::IoError("TransferParamsV1: too short".into())); } let fund_id = read_base(&data[0..32])?; let amount = u64::from_le_bytes(data[32..40].try_into().unwrap()); let recipient = PublicKey::from_bytes(data[40..72].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("TransferParamsV1: invalid recipient: {}", e)))?; let signature = read_base(&data[72..104])?; let exceeds_rate_limit = data[104] != 0; let has_vp = data[105] != 0; let vote_proposal_id = if has_vp { if data.len() != 138 { return Err(ContractError::IoError(format!("TransferParamsV1: expected 138 bytes, got {}", data.len()))); } Some(read_base(&data[106..138])?) } else { None }; Ok(TransferParamsV1 { fund_id, amount, recipient, signature, exceeds_rate_limit, vote_proposal_id }) } }

#[derive(Debug, Clone)] pub struct TransferUpdateV1 { pub amount: u64, pub recipient: PublicKey, pub rate_limited: bool }

#[derive(Debug, Clone,)]
pub struct LockParamsV1 { pub fund_id: FundId, pub duration_blocks: u64, pub signature: pallas::Base }
impl LockParamsV1 { pub const ENCODED_SIZE: usize = 72; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(72); b.extend_from_slice(&self.fund_id.to_repr()); b.extend_from_slice(&self.duration_blocks.to_le_bytes()); b.extend_from_slice(&self.signature.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 72 { return Err(ContractError::IoError(format!("LockParamsV1: expected 72 bytes, got {}", data.len()))); } Ok(LockParamsV1 { fund_id: read_base(&data[0..32])?, duration_blocks: u64::from_le_bytes(data[32..40].try_into().unwrap()), signature: read_base(&data[40..72])? }) } }

#[derive(Debug, Clone)] pub struct LockUpdateV1 { pub locked_until: u64 }

#[derive(Debug, Clone,)] pub struct UnlockParamsV1 { pub fund_id: FundId, pub signature: pallas::Base }
impl UnlockParamsV1 { pub const ENCODED_SIZE: usize = 64; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.fund_id.to_repr()); b.extend_from_slice(&self.signature.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("UnlockParamsV1: expected 64 bytes, got {}", data.len()))); } Ok(UnlockParamsV1 { fund_id: read_base(&data[0..32])?, signature: read_base(&data[32..64])? }) } }

#[derive(Debug, Clone)] pub struct UnlockUpdateV1 { pub unlocked_at: u64 }

#[derive(Debug, Clone,)]
pub struct UpdateConfigParamsV1 { pub fund_id: FundId, pub rate_limit: Option<RateLimit>, pub multisig_group_id: Option<pallas::Base>, pub new_spend_authority: Option<PublicKey> }
impl UpdateConfigParamsV1 { pub fn encode(&self) -> Vec<u8> { let rl = if let Some(ref r) = self.rate_limit { dwow_serial::serialize(r) } else { vec![] }; let mut b = Vec::with_capacity(34+rl.len()); b.extend_from_slice(&self.fund_id.to_repr()); b.push(self.rate_limit.is_some() as u8); b.extend_from_slice(&rl); b.push(self.multisig_group_id.is_some() as u8); if let Some(v) = self.multisig_group_id { b.extend_from_slice(&v.to_repr()); } b.push(self.new_spend_authority.is_some() as u8); if let Some(v) = self.new_spend_authority { b.extend_from_slice(&v.to_bytes()); } b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 34 { return Err(ContractError::IoError("UpdateConfigParamsV1: too short".into())); } let fund_id = read_base(&data[0..32])?; let has_rl = data[32] != 0; let mut pos = 33; let rate_limit = if has_rl { let r: RateLimit = dwow_serial::deserialize(&data[pos..]).map_err(|e| ContractError::IoError(format!("UpdateConfigParamsV1: invalid rate_limit: {:?}", e)))?; pos += dwow_serial::serialize(&r).len(); Some(r) } else { None }; let has_mg = data[pos] != 0; pos += 1; let multisig_group_id = if has_mg { let v = read_base(&data[pos..pos+32])?; pos += 32; Some(v) } else { None }; let has_sa = data[pos] != 0; let new_spend_authority = if has_sa { if data.len() != pos+33 { return Err(ContractError::IoError(format!("UpdateConfigParamsV1: expected {} bytes, got {}", pos+33, data.len()))); } Some(PublicKey::from_bytes(data[pos+1..pos+33].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("UpdateConfigParamsV1: invalid new_spend_authority: {}", e)))?) } else { None }; Ok(UpdateConfigParamsV1 { fund_id, rate_limit, multisig_group_id, new_spend_authority }) } }

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
