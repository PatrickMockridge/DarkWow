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
use dwow_serial::{SerialDecodable, SerialEncodable};

/// DAO-Escrow unique identifier (hash of parameters)
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct DaoEscrowBulla(pub pallas::Base);
impl DaoEscrowBulla {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(x).into_option().map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn zero() -> Self { Self(pallas::Base::zero()) }
}

/// Membership note identifier
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct MembershipNote(pub pallas::Base);
impl MembershipNote {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(x).into_option().map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn zero() -> Self { Self(pallas::Base::zero()) }
}

// ============================================================================
// DAO-ESCROW MODES
// ============================================================================

/// Operating mode of the DAO-Escrow
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
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

// ============================================================================
// ENDOWMENT CONFIGURATION
// ============================================================================

/// Fee distribution configuration (used in TreasuryEndowment mode)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
    /// Derive the DAO-Escrow bulla from parameters.
    /// Formula must match init_v1.zk circuit: poseidon_hash(dao_bulla, ox, oy, pool_token_id, blind)
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateParamsV1 {
    /// DAO-Escrow bulla
    pub bulla: DaoEscrowBulla,
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EnableDrainProtectionParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// DrainProtection bulla (from DrainProtection::InitializeV1)
    pub drain_protection_bulla: DaoEscrowBulla,
}

/// State update for `EnableDrainProtectionV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ClaimId(pub pallas::Base);
impl ClaimId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(x).into_option().map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
}

/// Vote type for claims
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum VoteType {
    /// Yes vote
    Yes = 0,
    /// No vote
    No = 1,
}

impl TryFrom<u8> for VoteType {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Yes),
            1 => Ok(Self::No),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Parameters for proposing a claim (endowment, treasury, or dispute)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
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

// ============================================================================
// PROPOSAL
// ============================================================================

/// Proposal identifier type
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ProposalId(pub pallas::Base);
impl ProposalId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(x).into_option().map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
}

/// A governance proposal (claim against endowment or treasury spend)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Proposal {
    pub version: u8,
    /// Unique proposal identifier
    pub id: ProposalId,
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Proposer's public key
    pub proposer_pubkey: PublicKey,
    /// Type of claim
    pub claim_type: ClaimType,
    /// Value requested
    pub value: u64,
    /// Hash of the claim description
    pub description_hash: pallas::Base,
    /// Recipient public key
    pub recipient_pubkey: PublicKey,
    /// Number of yes votes
    pub yes_votes: u64,
    /// Number of no votes
    pub no_votes: u64,
    /// Current proposal state
    pub state: ProposalState,
    /// Block height when created
    pub created_at: u64,
    /// Block height when voting ends
    pub voting_ends_at: u64,
    /// Block height when execution window expires
    pub execution_deadline: u64,
}

// ============================================================================
// VOTE RECORD
// ============================================================================

/// A vote record (prevents double-voting via nullifier)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyMemberCapabilityParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Capability proof to verify
    pub capability_proof: CapabilityProof,
    /// Holder's public key
    pub holder_pubkey: PublicKey,
}

/// State update for `VerifyMemberCapabilityV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelClaimParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Proposer's public key (must match original proposer)
    pub proposer_pubkey: PublicKey,
}

/// State update for `CancelClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeactivateCapabilityRequirementParamsV1 {
    pub dao_escrow_bulla: DaoEscrowBulla,
    pub role: Vec<u8>,
}

/// State update for `DeactivateCapabilityRequirementV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeactivateCapabilityRequirementUpdateV1 {
    pub dao_escrow_bulla: DaoEscrowBulla,
    pub role: Vec<u8>,
}

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
// ============================================================================

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
