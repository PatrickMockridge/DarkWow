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

//! DAO-Escrow contract data structures
//!
//! ## Three Operating Modes
//!
//! DAO-Escrow supports three configuration modes via the `mode` field:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                         DAO-Escrow Modes                              │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │                                                                       │
//! │  MODE_ESCROW: Escrow-Only (Insurance Pool)                          │
//! │  ┌─────────────────────────────────────────────────────────────┐     │
//! │  │  - Members pay premiums → endowment grows                   │     │
//! │  │  - No treasury (operational funds)                         │     │
//! │  │  - Endowment pays out claims                                │     │
//! │  │  - For: Pure insurance, no overhead                        │     │
//! │  └─────────────────────────────────────────────────────────────┘     │
//! │                                                                       │
//! │  MODE_TREASURY: Treasury-Only (Same as DarkWow DAO)                  │
//! │  ┌─────────────────────────────────────────────────────────────┐     │
//! │  │  - Members pay fees → treasury grows                        │     │
//! │  │  - DAO votes on treasury spending                           │     │
//! │  │  - No endowment/insurance                                   │     │
//! │  │  - For: Protocol treasury, grants, development             │     │
//! │  └─────────────────────────────────────────────────────────────┘     │
//! │                                                                       │
//! │  MODE_TREASURY_ENDOWMENT: Treasury + Endowment (Combined)           │
//! │  ┌─────────────────────────────────────────────────────────────┐     │
//! │  │  Treasury:  │  Endowment:                                    │     │
//! │  │  - Operational │  - Insurance reserve                       │     │
//! │  │  - DAO votes   │  - Emergency only                          │     │
//! │  │  - Grants      │  - Cannot fund treasury                    │     │
//! │  │  - Dev costs   │  - Refund protection                      │     │
//! │  └─────────────────────────────────────────────────────────────┘     │
//! │  For: Full-featured DAO with insurance backing                     │
//! │                                                                       │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Fee Split (MODE_TREASURY_ENDOWMENT only)
//!
//! When a member pays a premium:
//! - `treasury_share` → Treasury (operational funds)
//! - `endowment_share` → Endowment (insurance reserve)
//!
//! The split is enforced in the circuit. In other modes, all funds go
//! to the single pool.

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, BaseBlind, IntentNullifier, PublicKey, ScalarBlind, TokenId},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};

/// DAO-Escrow unique identifier (hash of parameters)
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct DaoEscrowBulla(pub pallas::Base);
impl DaoEscrowBulla {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        Option::<pallas::Base>::from(pallas::Base::from_repr(x)).map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn zero() -> Self { Self(pallas::Base::zero()) }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("expected 32 got {}", data.len()))); }
        Self::from_bytes(data.try_into().unwrap()).ok_or_else(|| ContractError::IoError("invalid field element".into()))
    }
}

/// Membership note identifier
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MembershipNote(pub pallas::Base);
impl MembershipNote {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        Option::<pallas::Base>::from(pallas::Base::from_repr(x)).map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn zero() -> Self { Self(pallas::Base::zero()) }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("expected 32 got {}", data.len()))); }
        Self::from_bytes(data.try_into().unwrap()).ok_or_else(|| ContractError::IoError("invalid field element".into()))
    }
}

// ============================================================================
// DAO-ESCROW MODES
// ============================================================================

/// Operating mode of the DAO-Escrow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaoEscrowMode {
    /// Escrow-only: Pure insurance pool, no treasury
    Escrow = 0,
    /// Treasury-only: Same as DarkWow DAO, no endowment
    Treasury = 1,
    /// Treasury + Endowment: Full-featured with insurance backing
    TreasuryEndowment = 2,
}

impl TryFrom<u8> for DaoEscrowMode {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Escrow),
            1 => Ok(Self::Treasury),
            2 => Ok(Self::TreasuryEndowment),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

impl DaoEscrowMode { pub fn encode(&self) -> Vec<u8> { vec![*self as u8] } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.is_empty() { return Err(ContractError::IoError("DaoEscrowMode: empty".into())); } Self::try_from(data[0]) } }

// ============================================================================
// ENDOWMENT CONFIGURATION
// ============================================================================

/// Fee distribution configuration (used in TreasuryEndowment mode)
#[derive(Debug, Clone)]
pub struct FeeConfig {
    pub version: u8,
    /// Treasury share (percentage * 10000, e.g., 7000 = 70%)
    pub treasury_share: u32,
    /// Endowment share (percentage * 10000, e.g., 3000 = 30%)
    pub endowment_share: u32,
}

// GovernanceConfig replaced by MultiSig composition — see multisig_group_id
// on DaoEscrow struct. Voting, quorum, approval ratios delegated to MultiSig groups.

/// Represents a DAO-Escrow instance
#[derive(Debug, Clone)]
pub struct DaoEscrow {
    pub version: u8,
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Bulla (unique identifier)
    pub bulla: DaoEscrowBulla,
    /// Operating mode
    pub mode: DaoEscrowMode,
    /// Owner/creator public key
    pub owner_pubkey: PublicKey,
    /// Token ID held in the pool
    pub pool_token_id: TokenId,
    /// MultiSig group ID for governance voting (replaces GovernanceConfig)
    pub multisig_group_id: pallas::Base,
    /// Purse instance ID for pool balance (replaces total_pool)
    pub pool_purse_id: pallas::Base,
    /// Purse instance ID for treasury balance (replaces total_treasury)
    pub treasury_purse_id: pallas::Base,
    /// Purse instance ID for endowment balance (replaces total_endowment)
    pub endowment_purse_id: pallas::Base,
    /// Number of active members
    pub member_count: u64,
    /// Fee configuration (TreasuryEndowment mode only)
    pub fee_config: Option<FeeConfig>,
    /// Minimum premium amount
    pub min_premium: u64,
    /// Maximum members allowed
    pub max_members: u64,
    /// Creation block
    pub created_at: u64,
    /// Bulla blind factor
    pub bulla_blind: BaseBlind,
    /// Whether governance is paused
    pub paused: bool,
    /// Whether DrainProtection is enabled for this instance
    pub drain_protection_enabled: bool,
    /// Associated DrainProtection bulla (if enabled)
    pub drain_protection_bulla: Option<DaoEscrowBulla>,
}

impl DaoEscrow {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(367);
        b.push(self.version);
        b.extend_from_slice(&self.instance_seed);
        b.extend_from_slice(&self.bulla.to_bytes());
        b.push(self.mode as u8);
        b.extend_from_slice(&self.owner_pubkey.to_bytes());
        b.extend_from_slice(&self.pool_token_id.to_bytes());
        b.extend_from_slice(&self.multisig_group_id.to_repr());
        b.extend_from_slice(&self.pool_purse_id.to_repr());
        b.extend_from_slice(&self.treasury_purse_id.to_repr());
        b.extend_from_slice(&self.endowment_purse_id.to_repr());
        b.extend_from_slice(&self.member_count.to_le_bytes());
        b.push(self.fee_config.is_some() as u8);
        if let Some(ref fc) = self.fee_config { b.extend_from_slice(&fc.encode()); }
        b.extend_from_slice(&self.min_premium.to_le_bytes());
        b.extend_from_slice(&self.max_members.to_le_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.bulla_blind.inner().to_repr());
        b.push(self.paused as u8);
        b.push(self.drain_protection_enabled as u8);
        b.push(self.drain_protection_bulla.is_some() as u8);
        if let Some(ref db) = self.drain_protection_bulla { b.extend_from_slice(&db.to_bytes()); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 326 { return Err(ContractError::IoError(format!("DaoEscrow: expected at least 326 bytes, got {}", data.len()))); }
        let version = data[0];
        let instance_seed: [u8; 32] = data[1..33].try_into().unwrap();
        let bulla = DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DaoEscrow: invalid bulla".into()))?);
        let mode = DaoEscrowMode::try_from(data[65])?;
        let owner_pubkey = PublicKey::from_bytes(data[66..98].try_into().unwrap())?;
        let pool_token_id = TokenId::from_bytes(data[98..130].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("DaoEscrow: invalid pool_token_id: {}", e)))?;
        let multisig_group_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[130..162].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DaoEscrow: invalid multisig_group_id".into()))?;
        let pool_purse_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[162..194].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DaoEscrow: invalid pool_purse_id".into()))?;
        let treasury_purse_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[194..226].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DaoEscrow: invalid treasury_purse_id".into()))?;
        let endowment_purse_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[226..258].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DaoEscrow: invalid endowment_purse_id".into()))?;
        let member_count = u64::from_le_bytes(data[258..266].try_into().unwrap());
        let has_fc = data[266] != 0;
        let (fee_config, mut pos) = if has_fc { (Some(FeeConfig::decode(&data[267..276])?), 276usize) } else { (None, 267usize) };
        let min_premium = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let max_members = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let created_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let bulla_blind = dwow_sdk::crypto::Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DaoEscrow: invalid bulla_blind".into()))?);
        pos += 32;
        let paused = data[pos] != 0; pos += 1;
        let drain_protection_enabled = data[pos] != 0; pos += 1;
        let has_db = data[pos] != 0;
        let drain_protection_bulla = if has_db { Some(DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+1..pos+33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DaoEscrow: invalid drain_protection_bulla".into()))?)) } else { None };
        Ok(DaoEscrow { version, instance_seed, bulla, mode, owner_pubkey, pool_token_id, multisig_group_id, pool_purse_id, treasury_purse_id, endowment_purse_id, member_count, fee_config, min_premium, max_members, created_at, bulla_blind, paused, drain_protection_enabled, drain_protection_bulla })
    }
    /// Derive the DAO-Escrow bulla from parameters.
    pub fn derive_bulla(
        dao_bulla: DaoEscrowBulla,
        owner_pubkey: &PublicKey,
        pool_token_id: TokenId,
        bulla_blind: BaseBlind,
    ) -> DaoEscrowBulla {
        let (ox, oy) = owner_pubkey.xy().expect("pk not identity");
        DaoEscrowBulla(poseidon_hash([dao_bulla.inner(), ox, oy, pool_token_id.inner(), bulla_blind.inner()]))
    }
}

// ============================================================================
// MEMBERSHIP NOTE
// ============================================================================

/// Represents a membership note (time-limited)
#[derive(Debug, Clone)]
pub struct Membership {
    pub version: u8,
    /// Membership note (unique identifier)
    pub note: MembershipNote,
    /// DAO-Escrow bulla this membership belongs to
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Member's public key
    pub member_pubkey: PublicKey,
    /// Value/maturity of membership
    pub value: u64,
    /// Token ID
    pub token_id: TokenId,
    /// Expiry block (membership valid until this block)
    pub expiry: u64,
    /// Created at block
    pub created_at: u64,
}

impl Membership {
    /// Derive the membership note from parameters
    pub fn derive_note(
        dao_escrow_bulla: DaoEscrowBulla,
        member_pubkey: &PublicKey,
        value: u64,
        token_id: pallas::Base,
        expiry: u64,
        blind: BaseBlind,
    ) -> MembershipNote {
        let (mx, my) = member_pubkey.xy().expect("pk not identity");
        MembershipNote(poseidon_hash([
            dao_escrow_bulla.inner(),
            mx,
            my,
            pallas::Base::from(value),
            token_id,
            pallas::Base::from(expiry),
            blind.inner(),
        ]))
    }
}

// ============================================================================
// PARAMETERS (for contract calls)
// ============================================================================

/// Parameters for `DaoEscrow::InitializeV1`
#[derive(Debug, Clone, )]
pub struct InitializeParamsV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// The controlling DAO's bulla
    pub dao_bulla: DaoEscrowBulla,
    /// Owner's public key
    pub owner_pubkey: PublicKey,
    /// Endowment token ID
    pub endowment_token_id: TokenId,
    /// Bulla blind factor
    pub bulla_blind: BaseBlind,
    /// Enable DrainProtection for this instance
    /// When true, endowment/treasury transfers are rate-limited and require
    /// 2/3 vote for large withdrawals. Member exit has 1/3 haircut.
    pub enable_drain_protection: bool,
}

/// State update for `DaoEscrow::InitializeV1`
#[derive(Debug, Clone)]
pub struct InitializeUpdateV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// The created endowment bulla
    pub bulla: DaoEscrowBulla,
    /// Owner public key (for withdrawal authorization)
    pub owner_pubkey: PublicKey,
    /// Bulla blind factor
    pub bulla_blind: BaseBlind,
}

/// Parameters for `DaoEscrow::UpdateV1`
#[derive(Debug, Clone, )]
pub struct UpdateParamsV1 {
    /// DAO-Escrow bulla
    pub bulla: DaoEscrowBulla,
}

/// State update for `DaoEscrow::UpdateV1`
#[derive(Debug, Clone)]
pub struct UpdateUpdateV1 {
    /// Updated DAO-Escrow bulla
    pub bulla: DaoEscrowBulla,
}

/// Parameters for `DaoEscrow::PayPremiumV1`
#[derive(Debug, Clone, )]
pub struct PayPremiumParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Membership note commitment
    pub membership_note: MembershipNote,
    /// Member's value commitment (Pedersen)
    pub value_commit: pallas::Point,
    /// Premium amount being paid
    pub value: u64,
    /// Token ID
    pub token_id: TokenId,
    /// Membership expiry block
    pub expiry: u64,
    /// Membership blind factor
    pub membership_blind: BaseBlind,
    /// Value blind factor
    pub value_blind: ScalarBlind,
    /// Member public key (verified in ZK proof)
    pub member_pubkey: PublicKey,
}

/// State update for `DaoEscrow::PayPremiumV1`
#[derive(Debug, Clone)]
pub struct PayPremiumUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Created membership note
    pub membership_note: MembershipNote,
    /// Updated total endowment
    pub amount: u64,
    /// Updated member count
    pub member_count: u64,
    /// Member public key
    pub member_pubkey: PublicKey,
    /// Token ID
    pub token_id: TokenId,
    /// Membership expiry block
    pub expiry: u64,
}

/// Parameters for `DaoEscrow::WithdrawV1`
#[derive(Debug, Clone, )]
pub struct WithdrawParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Amount to withdraw
    pub value: u64,
    /// Recipient
    pub recipient_pubkey: PublicKey,
    /// Optional capability proof for governance-based withdrawal
    pub capability_proof: Option<CapabilityProof>,
}

/// State update for `DaoEscrow::WithdrawV1`
#[derive(Debug, Clone)]
pub struct WithdrawUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Withdrawn amount
    pub value: u64,
    /// Updated total endowment
    pub amount: u64,
}

// ============================================================================
// DRAIN PROTECTION INTEGRATION
// ============================================================================

/// Parameters for enabling DrainProtection on an existing DAO-Escrow
#[derive(Debug, Clone, )]
pub struct EnableDrainProtectionParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// DrainProtection bulla (from DrainProtection::InitializeV1)
    pub drain_protection_bulla: DaoEscrowBulla,
}

/// State update for `EnableDrainProtectionV1`
#[derive(Debug, Clone)]
pub struct EnableDrainProtectionUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// DrainProtection bulla now associated
    pub drain_protection_bulla: DaoEscrowBulla,
}

// ============================================================================
// CLAIM / ENDOWMENT WITHDRAWAL TYPES (for EndowmentWithdrawV1)
// ============================================================================

/// Claim identifier
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ClaimId(pub pallas::Base);
impl ClaimId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        Option::<pallas::Base>::from(pallas::Base::from_repr(x)).map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("expected 32 got {}", data.len()))); }
        Self::from_bytes(data.try_into().unwrap()).ok_or_else(|| ContractError::IoError("invalid field element".into()))
    }
}

/// Vote type for claims
#[derive(Debug, Clone, Copy, PartialEq, Eq, )]
pub enum VoteType {
    /// Yes vote
    Yes = 0,
    /// No vote
    No = 1,
}

impl TryFrom<u8> for VoteType { type Error = dwow_sdk::error::ContractError; fn try_from(b: u8) -> Result<Self, Self::Error> { match b { 0=>Ok(Self::Yes),1=>Ok(Self::No),_=>Err(dwow_sdk::error::ContractError::InvalidFunction) } } }
impl VoteType { pub fn encode(&self) -> Vec<u8> { vec![*self as u8] } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.is_empty() { return Err(ContractError::IoError("VoteType: empty".into())); } Self::try_from(data[0]) } }

impl TryFrom<u8> for ClaimType { type Error = dwow_sdk::error::ContractError; fn try_from(b: u8) -> Result<Self, Self::Error> { match b { 0=>Ok(Self::Endowment),1=>Ok(Self::Treasury),2=>Ok(Self::Dispute),_=>Err(dwow_sdk::error::ContractError::InvalidFunction) } } }
impl ClaimType { pub fn encode(&self) -> Vec<u8> { vec![*self as u8] } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.is_empty() { return Err(ContractError::IoError("ClaimType: empty".into())); } Self::try_from(data[0]) } }

/// Parameters for proposing a claim (endowment, treasury, or dispute)
#[derive(Debug, Clone, )]
pub struct ProposeClaimParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Amount being claimed
    pub value: u64,
    /// Description hash
    pub description_hash: pallas::Base,
    /// Recipient public key
    pub recipient_pubkey: PublicKey,
    /// Proposer public key
    pub proposer_pubkey: PublicKey,
    /// Claim type (endowment, treasury, dispute)
    pub claim_type: ClaimType,
    /// Capability proof for member_vote
    pub capability_proof: CapabilityProof,
}

/// State update for `ProposeClaimV1`
#[derive(Debug, Clone)]
pub struct ProposeClaimUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Amount being claimed
    pub value: u64,
    /// Voting deadline
    pub voting_ends_at: u64,
    /// Execution deadline
    pub execution_deadline: u64,
    /// Proposer public key
    pub proposer_pubkey: PublicKey,
    /// Recipient public key
    pub recipient_pubkey: PublicKey,
    /// Claim type
    pub claim_type: ClaimType,
    /// Description hash
    pub description_hash: pallas::Base,
}

/// Parameters for voting on a claim
#[derive(Debug, Clone, )]
pub struct VoteClaimParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Vote type
    pub vote: VoteType,
    /// Voter's public key
    pub voter_pubkey: PublicKey,
    /// Capability proof for member_vote
    pub capability_proof: CapabilityProof,
}

/// State update for `VoteClaimV1`
#[derive(Debug, Clone)]
pub struct VoteClaimUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Updated vote tally
    pub yes_votes: u64,
    pub no_votes: u64,
    /// Whether proposal passed (met quorum and approval ratio)
    pub passed: bool,
    /// Whether proposal expired (voting window elapsed)
    pub expired: bool,
}

// ============================================================================
// ENDOWMENT WITHDRAWAL (Execute approved claim)
// ============================================================================

/// Parameters for executing an approved endowment withdrawal (claim)
#[derive(Debug, Clone, )]
pub struct EndowmentWithdrawParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier (must have been approved by DAO vote)
    pub claim_id: ClaimId,
    /// Recipient of the funds
    pub recipient_pubkey: PublicKey,
    /// Amount to withdraw
    pub value: u64,
    /// Optional capability proof for governance-based withdrawal
    pub capability_proof: Option<CapabilityProof>,
    /// Optional proposal ID (if approved by governance vote)
    pub proposal_id: Option<pallas::Base>,
}

/// State update for `EndowmentWithdrawV1`
#[derive(Debug, Clone)]
pub struct EndowmentWithdrawUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Amount withdrawn
    pub value: u64,
    /// Updated total endowment
    pub amount: u64,
}

// ============================================================================
// TREASURY SPEND (Execute approved treasury proposal)
// ============================================================================

/// Parameters for executing an approved treasury spend
#[derive(Debug, Clone, )]
pub struct TreasurySpendParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Proposal identifier
    pub proposal_id: pallas::Base,
    /// Recipient of the funds
    pub recipient_pubkey: PublicKey,
    /// Amount to spend
    pub value: u64,
    /// Optional capability proof for governance-based spending
    pub capability_proof: Option<CapabilityProof>,
}

/// State update for `TreasurySpendV1`
#[derive(Debug, Clone)]
pub struct TreasurySpendUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Proposal identifier
    pub proposal_id: pallas::Base,
    /// Amount spent
    pub value: u64,
    /// Updated total treasury
    pub amount: u64,
}

// ============================================================================
// CAPABILITY PROOF (cross-contract reference to Identity contract)
// ============================================================================

/// A capability proof from the Identity contract.
/// Referenced by dao_escrow to verify capability-based authorization.
#[derive(Debug, Clone, )]
pub struct CapabilityProof {
    /// Capability identifier (from Identity contract)
    pub capability_id: [u8; 32],
    /// Holder's capability secret
    pub capability_secret: [u8; 32],
    /// Nullifier to prevent replay
    pub nullifier: IntentNullifier,
    /// Issuer's public key
    pub issuer_pub: [u8; 32],
    /// Predicate result from ZK circuit
    pub predicate_result: [u8; 32],
    /// ZK proof bytes
    pub proof: Vec<u8>,
}

// ============================================================================
// CAPABILITY REQUIREMENT (maps DAO role to required capability)
// ============================================================================

/// Maps a DAO role to a required capability ID from the Identity contract
#[derive(Debug, Clone)]
pub struct CapabilityRequirement {
    pub version: u8,
    /// Role name (e.g., "member_vote", "board_treasury")
    pub role: Vec<u8>,
    /// Required capability ID from the Identity contract
    pub capability_id: [u8; 32],
    /// The Identity contract bulla (for cross-contract verification)
    pub identity_contract_bulla: pallas::Base,
    /// Whether this requirement is currently active
    pub active: bool,
}

// ============================================================================
// CLAIM TYPE
// ============================================================================

/// Type of claim or proposal
#[derive(Debug, Clone, Copy, PartialEq, Eq, )]
pub enum ClaimType {
    /// Claim against endowment (insurance)
    Endowment = 0,
    /// Treasury spend proposal
    Treasury = 1,
    /// Dispute resolution via oracle
    Dispute = 2,
}

// ============================================================================
// PROPOSAL STATE
// ============================================================================

/// Proposal lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    /// Voting is open
    Pending = 0,
    /// Vote passed
    Approved = 1,
    /// Vote failed
    Rejected = 2,
    /// Claim has been executed
    Executed = 3,
    /// Proposer cancelled
    Cancelled = 4,
    /// Voting/execution window expired
    Expired = 5,
}

impl TryFrom<u8> for ProposalState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Approved),
            2 => Ok(Self::Rejected),
            3 => Ok(Self::Executed),
            4 => Ok(Self::Cancelled),
            5 => Ok(Self::Expired),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// PROPOSAL
// ============================================================================

/// Proposal identifier type
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ProposalId(pub pallas::Base);
impl ProposalId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        Option::<pallas::Base>::from(pallas::Base::from_repr(x)).map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("expected 32 got {}", data.len()))); }
        Self::from_bytes(data.try_into().unwrap()).ok_or_else(|| ContractError::IoError("invalid field element".into()))
    }
}

/// A governance proposal (claim against endowment or treasury spend)
#[derive(Debug, Clone)]
pub struct Proposal {
    pub version: u8,
    pub id: ProposalId,
    pub dao_escrow_bulla: DaoEscrowBulla,
    pub proposer_pubkey: PublicKey,
    pub claim_type: ClaimType,
    pub value: u64,
    pub description_hash: pallas::Base,
    pub recipient_pubkey: PublicKey,
    pub yes_votes: u64,
    pub no_votes: u64,
    pub state: ProposalState,
    pub created_at: u64,
    pub voting_ends_at: u64,
    pub execution_deadline: u64,
}

impl Proposal {
    pub const ENCODED_SIZE: usize = 211;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(211);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_bytes());
        b.extend_from_slice(&self.dao_escrow_bulla.to_bytes());
        b.extend_from_slice(&self.proposer_pubkey.to_bytes());
        b.push(self.claim_type as u8);
        b.extend_from_slice(&self.value.to_le_bytes());
        b.extend_from_slice(&self.description_hash.to_repr());
        b.extend_from_slice(&self.recipient_pubkey.to_bytes());
        b.extend_from_slice(&self.yes_votes.to_le_bytes());
        b.extend_from_slice(&self.no_votes.to_le_bytes());
        b.push(self.state as u8);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.voting_ends_at.to_le_bytes());
        b.extend_from_slice(&self.execution_deadline.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 211 { return Err(ContractError::IoError(format!("Proposal: expected 211 bytes, got {}", data.len()))); }
        Ok(Proposal {
            version: data[0],
            id: ProposalId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Proposal: invalid id".into()))?),
            dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Proposal: invalid dao_escrow_bulla".into()))?),
            proposer_pubkey: PublicKey::from_bytes(data[65..97].try_into().unwrap())?,
            claim_type: ClaimType::try_from(data[97])?,
            value: u64::from_le_bytes(data[98..106].try_into().unwrap()),
            description_hash: Option::<pallas::Base>::from(pallas::Base::from_repr(data[106..138].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Proposal: invalid description_hash".into()))?,
            recipient_pubkey: PublicKey::from_bytes(data[138..170].try_into().unwrap())?,
            yes_votes: u64::from_le_bytes(data[170..178].try_into().unwrap()),
            no_votes: u64::from_le_bytes(data[178..186].try_into().unwrap()),
            state: ProposalState::try_from(data[186])?,
            created_at: u64::from_le_bytes(data[187..195].try_into().unwrap()),
            voting_ends_at: u64::from_le_bytes(data[195..203].try_into().unwrap()),
            execution_deadline: u64::from_le_bytes(data[203..211].try_into().unwrap()),
        })
    }
}

// ============================================================================
// VOTE RECORD
// ============================================================================

/// A vote record (prevents double-voting via nullifier)
#[derive(Debug, Clone)]
pub struct VoteRecord {
    pub version: u8,
    /// Proposal being voted on
    pub proposal_id: ProposalId,
    /// Voter's public key
    pub voter_pubkey: PublicKey,
    /// Vote direction
    pub vote: VoteType,
    /// Block height when voted
    pub voted_at: u64,
    /// Vote nullifier (prevents double-vote: H(capability_secret, proposal_id))
    pub vote_nullifier: pallas::Base,
}

// ============================================================================
// ORACLE ATTESTATION REFERENCE
// ============================================================================

/// Reference to an oracle attestation used for dispute resolution
#[derive(Debug, Clone, )]
pub struct OracleAttestationRef {
    pub version: u8,
    /// Attestation ID from the attestation contract
    pub attestation_id: pallas::Base,
    /// Oracle ID from the oracle contract
    pub oracle_id: pallas::Base,
    /// The attested value
    pub attested_value: pallas::Base,
    /// Block height when attested
    pub attested_at: u64,
}

// ============================================================================
// DISPUTE RESOLUTION
// ============================================================================

/// Dispute resolution record
#[derive(Debug, Clone)]
pub struct DisputeResolution {
    pub version: u8,
    /// Unique dispute identifier
    pub id: pallas::Base,
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Associated proposal ID
    pub proposal_id: ProposalId,
    /// Oracle attestations used for resolution
    pub attestations: Vec<OracleAttestationRef>,
    /// Resolution result (true = payout approved)
    pub resolution_result: bool,
    /// Payout amount
    pub payout_amount: u64,
    /// Payout recipient
    pub payout_recipient: PublicKey,
    /// Block height when resolved
    pub resolved_at: u64,
    /// Whether the payout has been executed
    pub executed: bool,
}

// ============================================================================
// EXECUTE CLAIM V1
// ============================================================================

/// Parameters for `ExecuteClaimV1`
#[derive(Debug, Clone, )]
pub struct ExecuteClaimParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Proposal ID to execute
    pub proposal_id: ProposalId,
    /// Recipient of the funds
    pub recipient_pubkey: PublicKey,
    /// Amount to transfer
    pub value: u64,
}

/// State update for `ExecuteClaimV1`
#[derive(Debug, Clone)]
pub struct ExecuteClaimUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Proposal ID
    pub proposal_id: ProposalId,
    /// Amount executed
    pub value: u64,
    /// Updated state
    pub state: ProposalState,
}

// ============================================================================
// REGISTER CAPABILITY REQUIREMENT V1
// ============================================================================

/// Parameters for `RegisterCapabilityRequirementV1`
#[derive(Debug, Clone, )]
pub struct RegisterCapabilityRequirementParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Role name
    pub role: Vec<u8>,
    /// Required capability ID
    pub capability_id: [u8; 32],
    /// Identity contract bulla reference
    pub identity_contract_bulla: pallas::Base,
}

/// State update for `RegisterCapabilityRequirementV1`
#[derive(Debug, Clone)]
pub struct RegisterCapabilityRequirementUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Role name
    pub role: Vec<u8>,
    /// Registered capability requirement
    pub requirement: CapabilityRequirement,
}

// ============================================================================
// VERIFY MEMBER CAPABILITY V1
// ============================================================================

/// Parameters for `VerifyMemberCapabilityV1`
#[derive(Debug, Clone, )]
pub struct VerifyMemberCapabilityParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Capability proof to verify
    pub capability_proof: CapabilityProof,
    /// Holder's public key
    pub holder_pubkey: PublicKey,
}

/// State update for `VerifyMemberCapabilityV1`
#[derive(Debug, Clone)]
pub struct VerifyMemberCapabilityUpdateV1 {
    /// Capability ID that was verified
    pub capability_id: [u8; 32],
    /// Whether verification passed
    pub verified: bool,
}

// ============================================================================
// RESOLVE DISPUTE V1
// ============================================================================

/// Parameters for `ResolveDisputeV1`
#[derive(Debug, Clone, )]
pub struct ResolveDisputeParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Associated proposal ID
    pub proposal_id: ProposalId,
    /// Oracle attestations for resolution
    pub attestations: Vec<OracleAttestationRef>,
    /// Arbitrator's capability proof (dispute_arbitrator)
    pub capability_proof: CapabilityProof,
    /// Payout amount
    pub payout_amount: u64,
    /// Payout recipient
    pub payout_recipient: PublicKey,
}

/// State update for `ResolveDisputeV1`
#[derive(Debug, Clone)]
pub struct ResolveDisputeUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Dispute ID
    pub dispute_id: pallas::Base,
    /// Proposal ID
    pub proposal_id: ProposalId,
    /// Whether payout was approved
    pub approved: bool,
    /// Payout amount
    pub payout_amount: u64,
    /// Attestation IDs consumed (prevents replay)
    pub consumed_attestation_ids: Vec<pallas::Base>,
}

// ============================================================================
// CANCEL CLAIM V1
// ============================================================================

/// Parameters for `CancelClaimV1`
#[derive(Debug, Clone, )]
pub struct CancelClaimParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Proposer's public key (must match original proposer)
    pub proposer_pubkey: PublicKey,
}

/// State update for `CancelClaimV1`
#[derive(Debug, Clone)]
pub struct CancelClaimUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Updated state
    pub state: ProposalState,
}

// ============================================================================
// SET GOVERNANCE CONFIG V1
// ============================================================================

// SetGovernanceConfigV1 + SetGovernanceActiveV1 removed — MultiSig groups
// manage governance configuration and activation.

/// Parameters for `DeactivateCapabilityRequirementV1`
#[derive(Debug, Clone, )]
pub struct DeactivateCapabilityRequirementParamsV1 {
    pub dao_escrow_bulla: DaoEscrowBulla,
    pub role: Vec<u8>,
}

/// State update for `DeactivateCapabilityRequirementV1`
#[derive(Debug, Clone)]
pub struct DeactivateCapabilityRequirementUpdateV1 {
    pub dao_escrow_bulla: DaoEscrowBulla,
    pub role: Vec<u8>,
}

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
// ============================================================================

// --- Parameter structs ---

impl dwow_serial::Encodable for InitializeParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for InitializeParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl InitializeParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(162); b.extend_from_slice(&self.instance_seed); b.extend_from_slice(&self.dao_bulla.to_bytes()); b.extend_from_slice(&self.owner_pubkey.to_bytes()); b.extend_from_slice(&self.endowment_token_id.to_bytes()); b.extend_from_slice(&self.bulla_blind.inner().to_repr()); b.push(self.enable_drain_protection as u8); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 162 { return Err(ContractError::IoError(format!("InitializeParamsV1: expected 162 got {}", data.len()))); } Ok(InitializeParamsV1 { instance_seed: data[0..32].try_into().unwrap(), dao_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("InitializeParamsV1: invalid dao_bulla".into()))?), owner_pubkey: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("InitializeParamsV1: invalid owner_pubkey: {}", e)))?, endowment_token_id: TokenId::from_bytes(data[96..128].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("InitializeParamsV1: invalid endowment_token_id: {}", e)))?, bulla_blind: dwow_sdk::crypto::Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(data[128..160].try_into().unwrap())).ok_or_else(|| ContractError::IoError("InitializeParamsV1: invalid bulla_blind".into()))?), enable_drain_protection: data[160] != 0 }) } }

impl dwow_serial::Encodable for UpdateParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for UpdateParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl UpdateParamsV1 { pub fn encode(&self) -> Vec<u8> { self.bulla.to_bytes().to_vec() } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("UpdateParamsV1: expected 32 got {}", data.len()))); } Ok(UpdateParamsV1 { bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UpdateParamsV1: invalid bulla".into()))?) }) } }

impl dwow_serial::Encodable for PayPremiumParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PayPremiumParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PayPremiumParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(216); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.membership_note.to_bytes()); b.extend_from_slice(&self.value_commit.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.token_id.to_bytes()); b.extend_from_slice(&self.expiry.to_le_bytes()); b.extend_from_slice(&self.membership_blind.inner().to_repr()); b.extend_from_slice(&self.value_blind.inner().to_repr()); b.extend_from_slice(&self.member_pubkey.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 216 { return Err(ContractError::IoError(format!("PayPremiumParamsV1: expected 216 got {}", data.len()))); } Ok(PayPremiumParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumParamsV1: invalid dao_escrow_bulla".into()))?), membership_note: MembershipNote(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumParamsV1: invalid membership_note".into()))?), value_commit: Option::<pallas::Point>::from(pallas::Point::from_bytes(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumParamsV1: invalid value_commit".into()))?, value: u64::from_le_bytes(data[96..104].try_into().unwrap()), token_id: TokenId::from_bytes(data[104..136].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PayPremiumParamsV1: invalid token_id: {}", e)))?, expiry: u64::from_le_bytes(data[136..144].try_into().unwrap()), membership_blind: dwow_sdk::crypto::Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(data[144..176].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumParamsV1: invalid membership_blind".into()))?), value_blind: dwow_sdk::crypto::Blind(Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(data[176..208].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumParamsV1: invalid value_blind".into()))?), member_pubkey: PublicKey::from_bytes(data[208..240].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PayPremiumParamsV1: invalid member_pubkey: {}", e)))? }) } }

impl dwow_serial::Encodable for WithdrawParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(97); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b.push(self.capability_proof.is_some() as u8); if let Some(ref cp) = self.capability_proof { b.extend_from_slice(&cp.encode()); } b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 73 { return Err(ContractError::IoError("WithdrawParamsV1: too short".into())); } let dao_escrow_bulla = DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("WithdrawParamsV1: invalid dao_escrow_bulla".into()))?); let value = u64::from_le_bytes(data[32..40].try_into().unwrap()); let recipient_pubkey = PublicKey::from_bytes(data[40..72].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("WithdrawParamsV1: invalid recipient_pubkey: {}", e)))?; let has_cp = data[72] != 0; let capability_proof = if has_cp { let cp = CapabilityProof::decode(&data[73..])?; Some(cp) } else { None }; Ok(WithdrawParamsV1 { dao_escrow_bulla, value, recipient_pubkey, capability_proof }) } }

impl dwow_serial::Encodable for EnableDrainProtectionParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for EnableDrainProtectionParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl EnableDrainProtectionParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.drain_protection_bulla.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("EnableDrainProtectionParamsV1: expected 64 got {}", data.len()))); } Ok(EnableDrainProtectionParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EnableDrainProtectionParamsV1: invalid dao_escrow_bulla".into()))?), drain_protection_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EnableDrainProtectionParamsV1: invalid drain_protection_bulla".into()))?) }) } }

impl ProposeClaimParamsV1 { pub fn encode(&self) -> Vec<u8> { let cp = self.capability_proof.encode(); let mut b = Vec::with_capacity(195+cp.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.description_hash.to_repr()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b.extend_from_slice(&self.proposer_pubkey.to_bytes()); b.push(self.claim_type as u8); b.extend_from_slice(&cp); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 195 { return Err(ContractError::IoError("ProposeClaimParamsV1: too short".into())); } let dao_escrow_bulla = DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ProposeClaimParamsV1: invalid dao_escrow_bulla".into()))?); let claim_id = ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ProposeClaimParamsV1: invalid claim_id".into()))?); let value = u64::from_le_bytes(data[64..72].try_into().unwrap()); let description_hash = Option::<pallas::Base>::from(pallas::Base::from_repr(data[72..104].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ProposeClaimParamsV1: invalid description_hash".into()))?; let recipient_pubkey = PublicKey::from_bytes(data[104..136].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ProposeClaimParamsV1: invalid recipient_pubkey: {}", e)))?; let proposer_pubkey = PublicKey::from_bytes(data[136..168].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ProposeClaimParamsV1: invalid proposer_pubkey: {}", e)))?; let claim_type = ClaimType::decode(&data[168..169])?; let capability_proof = CapabilityProof::decode(&data[169..])?; Ok(ProposeClaimParamsV1 { dao_escrow_bulla, claim_id, value, description_hash, recipient_pubkey, proposer_pubkey, claim_type, capability_proof }) } }

impl VoteClaimParamsV1 { pub fn encode(&self) -> Vec<u8> { let cp = self.capability_proof.encode(); let mut b = Vec::with_capacity(98+cp.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.push(self.vote as u8); b.extend_from_slice(&self.voter_pubkey.to_bytes()); b.extend_from_slice(&cp); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 98 { return Err(ContractError::IoError("VoteClaimParamsV1: too short".into())); } Ok(VoteClaimParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("VoteClaimParamsV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("VoteClaimParamsV1: invalid claim_id".into()))?), vote: VoteType::decode(&data[64..65])?, voter_pubkey: PublicKey::from_bytes(data[65..97].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("VoteClaimParamsV1: invalid voter_pubkey: {}", e)))?, capability_proof: CapabilityProof::decode(&data[97..])? }) } }

impl EndowmentWithdrawParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(138); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.push(self.capability_proof.is_some() as u8); if let Some(ref cp) = self.capability_proof { b.extend_from_slice(&cp.encode()); } b.push(self.proposal_id.is_some() as u8); if let Some(ref pid) = self.proposal_id { b.extend_from_slice(&pid.to_repr()); } b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 105 { return Err(ContractError::IoError("EndowmentWithdrawParamsV1: too short".into())); } let dao_escrow_bulla = DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EndowmentWithdrawParamsV1: invalid dao_escrow_bulla".into()))?); let claim_id = ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EndowmentWithdrawParamsV1: invalid claim_id".into()))?); let recipient_pubkey = PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("EndowmentWithdrawParamsV1: invalid recipient_pubkey: {}", e)))?; let value = u64::from_le_bytes(data[96..104].try_into().unwrap()); let has_cp = data[104] != 0; let mut pos = 105; let capability_proof = if has_cp { let cp = CapabilityProof::decode(&data[pos..])?; pos += cp.encode().len(); Some(cp) } else { None }; let has_pid = if pos < data.len() { data[pos] } else { 0 }; pos += 1; let proposal_id = if has_pid != 0 { if data.len() < pos+32 { return Err(ContractError::IoError("EndowmentWithdrawParamsV1: proposal_id truncated".into())); } let pid = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EndowmentWithdrawParamsV1: invalid proposal_id".into()))?; Some(pid) } else { None }; Ok(EndowmentWithdrawParamsV1 { dao_escrow_bulla, claim_id, recipient_pubkey, value, capability_proof, proposal_id }) } }

impl TreasurySpendParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(105); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.proposal_id.to_repr()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.push(self.capability_proof.is_some() as u8); if let Some(ref cp) = self.capability_proof { b.extend_from_slice(&cp.encode()); } b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 105 { return Err(ContractError::IoError("TreasurySpendParamsV1: too short".into())); } Ok(TreasurySpendParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("TreasurySpendParamsV1: invalid dao_escrow_bulla".into()))?), proposal_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("TreasurySpendParamsV1: invalid proposal_id".into()))?, recipient_pubkey: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("TreasurySpendParamsV1: invalid recipient_pubkey: {}", e)))?, value: u64::from_le_bytes(data[96..104].try_into().unwrap()), capability_proof: if data[104] != 0 { Some(CapabilityProof::decode(&data[105..])?) } else { None } }) } }

impl ResolveDisputeParamsV1 { pub fn encode(&self) -> Vec<u8> { let cp = self.capability_proof.encode(); let mut b = Vec::with_capacity(137+cp.len()+self.attestations.len()*105); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.proposal_id.to_bytes()); b.push(self.attestations.len() as u8); for a in &self.attestations { b.extend_from_slice(&a.encode()); } b.extend_from_slice(&cp); b.extend_from_slice(&self.payout_amount.to_le_bytes()); b.extend_from_slice(&self.payout_recipient.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 137 { return Err(ContractError::IoError("ResolveDisputeParamsV1: too short".into())); } let count = data[64] as usize; let mut pos = 65; let mut attestations = Vec::with_capacity(count); for _ in 0..count { attestations.push(OracleAttestationRef::decode(&data[pos..pos+105])?); pos += 105; } let remaining = &data[pos..]; let remaining_len = remaining.len(); if remaining_len < 40 { return Err(ContractError::IoError("ResolveDisputeParamsV1: trailing too short".into())); } let cp = CapabilityProof::decode(&remaining[..remaining_len-40])?; Ok(ResolveDisputeParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveDisputeParamsV1: invalid dao_escrow_bulla".into()))?), proposal_id: ProposalId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveDisputeParamsV1: invalid proposal_id".into()))?), attestations, capability_proof: cp, payout_amount: u64::from_le_bytes(remaining[remaining_len-40..remaining_len-32].try_into().unwrap()), payout_recipient: PublicKey::from_bytes(remaining[remaining_len-32..].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ResolveDisputeParamsV1: invalid payout_recipient: {}", e)))? }) } }

impl ExecuteClaimParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(104); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.proposal_id.to_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 104 { return Err(ContractError::IoError(format!("ExecuteClaimParamsV1: expected 104 got {}", data.len()))); } Ok(ExecuteClaimParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ExecuteClaimParamsV1: invalid dao_escrow_bulla".into()))?), proposal_id: ProposalId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ExecuteClaimParamsV1: invalid proposal_id".into()))?), recipient_pubkey: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ExecuteClaimParamsV1: invalid recipient_pubkey: {}", e)))?, value: u64::from_le_bytes(data[96..104].try_into().unwrap()) }) } }


impl CancelClaimParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(96); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.extend_from_slice(&self.proposer_pubkey.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 96 { return Err(ContractError::IoError(format!("CancelClaimParamsV1: expected 96 got {}", data.len()))); } Ok(CancelClaimParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CancelClaimParamsV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CancelClaimParamsV1: invalid claim_id".into()))?), proposer_pubkey: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CancelClaimParamsV1: invalid proposer_pubkey: {}", e)))? }) } }

impl RegisterCapabilityRequirementParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(65+self.role.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.push(self.role.len() as u8); b.extend_from_slice(&self.role); b.extend_from_slice(&self.capability_id); b.extend_from_slice(&self.identity_contract_bulla.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 65 { return Err(ContractError::IoError("RegisterCapabilityRequirementParamsV1: too short".into())); } let role_len = data[32] as usize; if data.len() != 65+role_len { return Err(ContractError::IoError(format!("RegisterCapabilityRequirementParamsV1: expected {} got {}", 65+role_len, data.len()))); } Ok(RegisterCapabilityRequirementParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RegisterCapabilityRequirementParamsV1: invalid dao_escrow_bulla".into()))?), role: data[33..33+role_len].to_vec(), capability_id: data[33+role_len..65+role_len].try_into().unwrap(), identity_contract_bulla: Option::<pallas::Base>::from(pallas::Base::from_repr(data[65+role_len..97+role_len].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RegisterCapabilityRequirementParamsV1: invalid identity_contract_bulla".into()))? }) } }

impl VerifyMemberCapabilityParamsV1 { pub fn encode(&self) -> Vec<u8> { let cp = self.capability_proof.encode(); let mut b = Vec::with_capacity(32+cp.len()+32); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&cp); b.extend_from_slice(&self.holder_pubkey.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 64 { return Err(ContractError::IoError("VerifyMemberCapabilityParamsV1: too short".into())); } let dao_escrow_bulla = DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("VerifyMemberCapabilityParamsV1: invalid dao_escrow_bulla".into()))?); let holder_pubkey = PublicKey::from_bytes(data[data.len()-32..].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("VerifyMemberCapabilityParamsV1: invalid holder_pubkey: {}", e)))?; let capability_proof = CapabilityProof::decode(&data[32..data.len()-32])?; Ok(VerifyMemberCapabilityParamsV1 { dao_escrow_bulla, capability_proof, holder_pubkey }) } }

impl DeactivateCapabilityRequirementParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(33+self.role.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.push(self.role.len() as u8); b.extend_from_slice(&self.role); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 33 { return Err(ContractError::IoError("DeactivateCapabilityRequirementParamsV1: too short".into())); } let role_len = data[32] as usize; if data.len() != 33+role_len { return Err(ContractError::IoError(format!("DeactivateCapabilityRequirementParamsV1: expected {} got {}", 33+role_len, data.len()))); } Ok(DeactivateCapabilityRequirementParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DeactivateCapabilityRequirementParamsV1: invalid dao_escrow_bulla".into()))?), role: data[33..33+role_len].to_vec() }) } }

impl CapabilityProof { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(161+self.proof.len()); b.extend_from_slice(&self.capability_id); b.extend_from_slice(&self.capability_secret); b.extend_from_slice(&self.nullifier.to_bytes()); b.extend_from_slice(&self.issuer_pub); b.extend_from_slice(&self.predicate_result); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 161 { return Err(ContractError::IoError("CapabilityProof: too short".into())); } let proof_len = data[160] as usize; if data.len() != 161+proof_len { return Err(ContractError::IoError(format!("CapabilityProof: expected {} got {}", 161+proof_len, data.len()))); } Ok(CapabilityProof { capability_id: data[0..32].try_into().unwrap(), capability_secret: data[32..64].try_into().unwrap(), nullifier: IntentNullifier::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CapabilityProof: invalid nullifier: {}", e)))?, issuer_pub: data[96..128].try_into().unwrap(), predicate_result: data[128..160].try_into().unwrap(), proof: data[161..161+proof_len].to_vec() }) } }

impl OracleAttestationRef { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(105); b.push(self.version); b.extend_from_slice(&self.attestation_id.to_repr()); b.extend_from_slice(&self.oracle_id.to_repr()); b.extend_from_slice(&self.attested_value.to_repr()); b.extend_from_slice(&self.attested_at.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 105 { return Err(ContractError::IoError(format!("OracleAttestationRef: expected 105 got {}", data.len()))); } Ok(OracleAttestationRef { version: data[0], attestation_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("OracleAttestationRef: invalid attestation_id".into()))?, oracle_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("OracleAttestationRef: invalid oracle_id".into()))?, attested_value: Option::<pallas::Base>::from(pallas::Base::from_repr(data[65..97].try_into().unwrap())).ok_or_else(|| ContractError::IoError("OracleAttestationRef: invalid attested_value".into()))?, attested_at: u64::from_le_bytes(data[97..105].try_into().unwrap()) }) } }

// --- Bridge update structs ---

impl dwow_serial::Encodable for UpdateUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for UpdateUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl UpdateUpdateV1 { pub const ENCODED_SIZE: usize = 32; pub fn encode(&self) -> Vec<u8> { self.bulla.to_bytes().to_vec() } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("UpdateUpdateV1: expected 32 bytes, got {}", data.len()))); } Ok(UpdateUpdateV1 { bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UpdateUpdateV1: invalid bulla".into()))?) }) } }

impl dwow_serial::Encodable for InitializeUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for InitializeUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl InitializeUpdateV1 { pub const ENCODED_SIZE: usize = 128; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(128); b.extend_from_slice(&self.instance_seed); b.extend_from_slice(&self.bulla.to_bytes()); b.extend_from_slice(&self.owner_pubkey.to_bytes()); b.extend_from_slice(&self.bulla_blind.inner().to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 128 { return Err(ContractError::IoError(format!("InitializeUpdateV1: expected 128 bytes, got {}", data.len()))); } Ok(InitializeUpdateV1 { instance_seed: data[0..32].try_into().unwrap(), bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("InitializeUpdateV1: invalid bulla".into()))?), owner_pubkey: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("InitializeUpdateV1: invalid owner_pubkey: {}", e)))?, bulla_blind: dwow_sdk::crypto::Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(data[96..128].try_into().unwrap())).ok_or_else(|| ContractError::IoError("InitializeUpdateV1: invalid bulla_blind".into()))?) }) } }

impl dwow_serial::Encodable for PayPremiumUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PayPremiumUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PayPremiumUpdateV1 { pub const ENCODED_SIZE: usize = 152; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(152); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.membership_note.to_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.member_count.to_le_bytes()); b.extend_from_slice(&self.member_pubkey.to_bytes()); b.extend_from_slice(&self.token_id.to_bytes()); b.extend_from_slice(&self.expiry.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 152 { return Err(ContractError::IoError(format!("PayPremiumUpdateV1: expected 152 bytes, got {}", data.len()))); } Ok(PayPremiumUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumUpdateV1: invalid dao_escrow_bulla".into()))?), membership_note: MembershipNote(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumUpdateV1: invalid membership_note".into()))?), amount: u64::from_le_bytes(data[64..72].try_into().unwrap()), member_count: u64::from_le_bytes(data[72..80].try_into().unwrap()), member_pubkey: PublicKey::from_bytes(data[80..112].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PayPremiumUpdateV1: invalid member_pubkey: {}", e)))?, token_id: TokenId::from_bytes(data[112..144].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PayPremiumUpdateV1: invalid token_id: {}", e)))?, expiry: u64::from_le_bytes(data[144..152].try_into().unwrap()) }) } }

impl dwow_serial::Encodable for WithdrawUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawUpdateV1 { pub const ENCODED_SIZE: usize = 48; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(48); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 48 { return Err(ContractError::IoError(format!("WithdrawUpdateV1: expected 48 bytes, got {}", data.len()))); } Ok(WithdrawUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("WithdrawUpdateV1: invalid dao_escrow_bulla".into()))?), value: u64::from_le_bytes(data[32..40].try_into().unwrap()), amount: u64::from_le_bytes(data[40..48].try_into().unwrap()) }) } }

impl EndowmentWithdrawUpdateV1 { pub const ENCODED_SIZE: usize = 80; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(80); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 80 { return Err(ContractError::IoError(format!("EndowmentWithdrawUpdateV1: expected 80 bytes, got {}", data.len()))); } Ok(EndowmentWithdrawUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EndowmentWithdrawUpdateV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EndowmentWithdrawUpdateV1: invalid claim_id".into()))?), value: u64::from_le_bytes(data[64..72].try_into().unwrap()), amount: u64::from_le_bytes(data[72..80].try_into().unwrap()) }) } }

impl TreasurySpendUpdateV1 { pub const ENCODED_SIZE: usize = 80; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(80); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.proposal_id.to_repr()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 80 { return Err(ContractError::IoError(format!("TreasurySpendUpdateV1: expected 80 bytes, got {}", data.len()))); } Ok(TreasurySpendUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("TreasurySpendUpdateV1: invalid dao_escrow_bulla".into()))?), proposal_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("TreasurySpendUpdateV1: invalid proposal_id".into()))?, value: u64::from_le_bytes(data[64..72].try_into().unwrap()), amount: u64::from_le_bytes(data[72..80].try_into().unwrap()) }) } }

impl dwow_serial::Encodable for EnableDrainProtectionUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for EnableDrainProtectionUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl EnableDrainProtectionUpdateV1 { pub const ENCODED_SIZE: usize = 64; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.drain_protection_bulla.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("EnableDrainProtectionUpdateV1: expected 64 bytes, got {}", data.len()))); } Ok(EnableDrainProtectionUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EnableDrainProtectionUpdateV1: invalid dao_escrow_bulla".into()))?), drain_protection_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EnableDrainProtectionUpdateV1: invalid drain_protection_bulla".into()))?) }) } }

impl ProposeClaimUpdateV1 { pub const ENCODED_SIZE: usize = 185; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(185); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.voting_ends_at.to_le_bytes()); b.extend_from_slice(&self.execution_deadline.to_le_bytes()); b.extend_from_slice(&self.proposer_pubkey.to_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b.push(self.claim_type as u8); b.extend_from_slice(&self.description_hash.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 185 { return Err(ContractError::IoError(format!("ProposeClaimUpdateV1: expected 185 bytes, got {}", data.len()))); } Ok(ProposeClaimUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ProposeClaimUpdateV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ProposeClaimUpdateV1: invalid claim_id".into()))?), value: u64::from_le_bytes(data[64..72].try_into().unwrap()), voting_ends_at: u64::from_le_bytes(data[72..80].try_into().unwrap()), execution_deadline: u64::from_le_bytes(data[80..88].try_into().unwrap()), proposer_pubkey: PublicKey::from_bytes(data[88..120].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ProposeClaimUpdateV1: invalid proposer_pubkey: {}", e)))?, recipient_pubkey: PublicKey::from_bytes(data[120..152].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ProposeClaimUpdateV1: invalid recipient_pubkey: {}", e)))?, claim_type: ClaimType::try_from(data[152])?, description_hash: Option::<pallas::Base>::from(pallas::Base::from_repr(data[153..185].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ProposeClaimUpdateV1: invalid description_hash".into()))? }) } }

impl VoteClaimUpdateV1 { pub const ENCODED_SIZE: usize = 82; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(82); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.extend_from_slice(&self.yes_votes.to_le_bytes()); b.extend_from_slice(&self.no_votes.to_le_bytes()); b.push(self.passed as u8); b.push(self.expired as u8); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 82 { return Err(ContractError::IoError(format!("VoteClaimUpdateV1: expected 82 bytes, got {}", data.len()))); } Ok(VoteClaimUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("VoteClaimUpdateV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("VoteClaimUpdateV1: invalid claim_id".into()))?), yes_votes: u64::from_le_bytes(data[64..72].try_into().unwrap()), no_votes: u64::from_le_bytes(data[72..80].try_into().unwrap()), passed: data[80] != 0, expired: data[81] != 0 }) } }

impl ExecuteClaimUpdateV1 { pub const ENCODED_SIZE: usize = 73; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(73); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.proposal_id.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.push(self.state as u8); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 73 { return Err(ContractError::IoError(format!("ExecuteClaimUpdateV1: expected 73 bytes, got {}", data.len()))); } Ok(ExecuteClaimUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ExecuteClaimUpdateV1: invalid dao_escrow_bulla".into()))?), proposal_id: ProposalId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ExecuteClaimUpdateV1: invalid proposal_id".into()))?), value: u64::from_le_bytes(data[64..72].try_into().unwrap()), state: ProposalState::try_from(data[72])? }) } }

impl VerifyMemberCapabilityUpdateV1 { pub const ENCODED_SIZE: usize = 33; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(33); b.extend_from_slice(&self.capability_id); b.push(self.verified as u8); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 33 { return Err(ContractError::IoError(format!("VerifyMemberCapabilityUpdateV1: expected 33 bytes, got {}", data.len()))); } Ok(VerifyMemberCapabilityUpdateV1 { capability_id: data[0..32].try_into().unwrap(), verified: data[32] != 0 }) } }

impl CancelClaimUpdateV1 { pub const ENCODED_SIZE: usize = 65; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(65); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.push(self.state as u8); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 65 { return Err(ContractError::IoError(format!("CancelClaimUpdateV1: expected 65 bytes, got {}", data.len()))); } Ok(CancelClaimUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CancelClaimUpdateV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CancelClaimUpdateV1: invalid claim_id".into()))?), state: ProposalState::try_from(data[64])? }) } }

impl FeeConfig {
    pub const ENCODED_SIZE: usize = 9;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(9); b.push(self.version); b.extend_from_slice(&self.treasury_share.to_le_bytes()); b.extend_from_slice(&self.endowment_share.to_le_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 9 { return Err(ContractError::IoError(format!("FeeConfig: expected 9 bytes, got {}", data.len()))); }
        Ok(FeeConfig { version: data[0], treasury_share: u32::from_le_bytes(data[1..5].try_into().unwrap()), endowment_share: u32::from_le_bytes(data[5..9].try_into().unwrap()) })
    }
}

impl Membership {
    pub const ENCODED_SIZE: usize = 153;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(153);
        b.push(self.version);
        b.extend_from_slice(&self.note.to_bytes());
        b.extend_from_slice(&self.dao_escrow_bulla.to_bytes());
        b.extend_from_slice(&self.member_pubkey.to_bytes());
        b.extend_from_slice(&self.value.to_le_bytes());
        b.extend_from_slice(&self.token_id.to_bytes());
        b.extend_from_slice(&self.expiry.to_le_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 153 { return Err(ContractError::IoError(format!("Membership: expected 153 bytes, got {}", data.len()))); }
        Ok(Membership { version: data[0], note: MembershipNote(Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Membership: invalid note".into()))?), dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Membership: invalid dao_escrow_bulla".into()))?), member_pubkey: PublicKey::from_bytes(data[65..97].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Membership: invalid member_pubkey: {}", e)))?, value: u64::from_le_bytes(data[97..105].try_into().unwrap()), token_id: TokenId::from_bytes(data[105..137].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Membership: invalid token_id: {}", e)))?, expiry: u64::from_le_bytes(data[137..145].try_into().unwrap()), created_at: u64::from_le_bytes(data[145..153].try_into().unwrap()) })
    }
}

impl CapabilityRequirement {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(35 + self.role.len());
        b.push(self.version);
        b.push(self.role.len() as u8);
        b.extend_from_slice(&self.role);
        b.extend_from_slice(&self.capability_id);
        b.extend_from_slice(&self.identity_contract_bulla.to_repr());
        b.push(self.active as u8);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 35 { return Err(ContractError::IoError(format!("CapabilityRequirement: expected at least 35 bytes, got {}", data.len()))); }
        let version = data[0];
        let role_len = data[1] as usize;
        if data.len() != 35 + role_len { return Err(ContractError::IoError(format!("CapabilityRequirement: expected {} bytes, got {}", 35 + role_len, data.len()))); }
        let role = data[2..2+role_len].to_vec();
        let capability_id: [u8; 32] = data[2+role_len..34+role_len].try_into().unwrap();
        let identity_contract_bulla = Option::<pallas::Base>::from(pallas::Base::from_repr(data[34+role_len..66+role_len].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CapabilityRequirement: invalid identity_contract_bulla".into()))?;
        let active = data[66+role_len] != 0;
        Ok(CapabilityRequirement { version, role, capability_id, identity_contract_bulla, active })
    }
}

impl RegisterCapabilityRequirementUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let inner = self.requirement.encode();
        let mut b = Vec::with_capacity(33 + self.role.len() + inner.len());
        b.extend_from_slice(&self.dao_escrow_bulla.to_bytes());
        b.push(self.role.len() as u8);
        b.extend_from_slice(&self.role);
        b.extend_from_slice(&inner);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 33 { return Err(ContractError::IoError(format!("RegisterCapabilityRequirementUpdateV1: expected at least 33 bytes, got {}", data.len()))); }
        let dao_escrow_bulla = DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RegisterCapabilityRequirementUpdateV1: invalid dao_escrow_bulla".into()))?);
        let role_len = data[32] as usize;
        if data.len() < 33 + role_len { return Err(ContractError::IoError("RegisterCapabilityRequirementUpdateV1: data too short".into())); }
        let role = data[33..33+role_len].to_vec();
        let requirement = CapabilityRequirement::decode(&data[33+role_len..])?;
        Ok(RegisterCapabilityRequirementUpdateV1 { dao_escrow_bulla, role, requirement })
    }
}

impl ResolveDisputeUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(106 + self.consumed_attestation_ids.len() * 32);
        b.extend_from_slice(&self.dao_escrow_bulla.to_bytes());
        b.extend_from_slice(&self.dispute_id.to_repr());
        b.extend_from_slice(&self.proposal_id.to_bytes());
        b.push(self.approved as u8);
        b.extend_from_slice(&self.payout_amount.to_le_bytes());
        b.push(self.consumed_attestation_ids.len() as u8);
        for id in &self.consumed_attestation_ids { b.extend_from_slice(&id.to_repr()); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 106 { return Err(ContractError::IoError(format!("ResolveDisputeUpdateV1: expected at least 106 bytes, got {}", data.len()))); }
        let dao_escrow_bulla = DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveDisputeUpdateV1: invalid dao_escrow_bulla".into()))?);
        let dispute_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveDisputeUpdateV1: invalid dispute_id".into()))?;
        let proposal_id = ProposalId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveDisputeUpdateV1: invalid proposal_id".into()))?);
        let approved = data[96] != 0;
        let payout_amount = u64::from_le_bytes(data[97..105].try_into().unwrap());
        let count = data[105] as usize;
        let expected = 106 + count * 32;
        if data.len() != expected { return Err(ContractError::IoError(format!("ResolveDisputeUpdateV1: expected {} bytes for {} ids, got {}", expected, count, data.len()))); }
        let mut consumed_attestation_ids = Vec::with_capacity(count);
        for i in 0..count { consumed_attestation_ids.push(Option::<pallas::Base>::from(pallas::Base::from_repr(data[106+i*32..106+(i+1)*32].try_into().unwrap())).ok_or_else(|| ContractError::IoError(format!("ResolveDisputeUpdateV1: invalid attestation_id[{}]", i)))?); }
        Ok(ResolveDisputeUpdateV1 { dao_escrow_bulla, dispute_id, proposal_id, approved, payout_amount, consumed_attestation_ids })
    }
}

impl DeactivateCapabilityRequirementUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(33 + self.role.len());
        b.extend_from_slice(&self.dao_escrow_bulla.to_bytes());
        b.push(self.role.len() as u8);
        b.extend_from_slice(&self.role);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 33 { return Err(ContractError::IoError(format!("DeactivateCapabilityRequirementUpdateV1: expected at least 33 bytes, got {}", data.len()))); }
        let dao_escrow_bulla = DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DeactivateCapabilityRequirementUpdateV1: invalid dao_escrow_bulla".into()))?);
        let role_len = data[32] as usize;
        if data.len() != 33 + role_len { return Err(ContractError::IoError(format!("DeactivateCapabilityRequirementUpdateV1: expected {} bytes, got {}", 33 + role_len, data.len()))); }
        let role = data[33..33+role_len].to_vec();
        Ok(DeactivateCapabilityRequirementUpdateV1 { dao_escrow_bulla, role })
    }
}
