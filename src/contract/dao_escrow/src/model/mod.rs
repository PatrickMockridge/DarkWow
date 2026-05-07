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
    crypto::{poseidon_hash, BaseBlind, PublicKey, ScalarBlind},
    pasta::pallas,
};
#[cfg(feature = "async")]
use dwow_serial::async_trait;
use dwow_serial::{SerialDecodable, SerialEncodable};

/// DAO-Escrow unique identifier (hash of parameters)
pub type DaoEscrowBulla = pallas::Base;

/// Membership note identifier
pub type MembershipNote = pallas::Base;

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
    /// Treasury share (percentage * 10000, e.g., 7000 = 70%)
    pub treasury_share: u32,
    /// Endowment share (percentage * 10000, e.g., 3000 = 30%)
    pub endowment_share: u32,
}

/// Represents a DAO-Escrow instance
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoEscrow {
    /// Bulla (unique identifier)
    pub bulla: DaoEscrowBulla,
    /// Operating mode
    pub mode: DaoEscrowMode,
    /// Owner/creator public key
    pub owner_pubkey: PublicKey,
    /// Token ID held in the pool
    pub pool_token_id: pallas::Base,
    /// Total pool value (treasury in Treasury mode, endowment in Escrow mode)
    pub total_pool: u64,
    /// Total treasury (TreasuryEndowment mode only)
    pub total_treasury: u64,
    /// Total endowment (TreasuryEndowment mode only)
    pub total_endowment: u64,
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
    /// Derive the DAO-Escrow bulla from parameters
    #[allow(dead_code)]
    pub fn derive_bulla(
        mode: DaoEscrowMode,
        owner_pubkey: &PublicKey,
        pool_token_id: pallas::Base,
        fee_config: &Option<FeeConfig>,
        bulla_blind: BaseBlind,
    ) -> DaoEscrowBulla {
        let (ox, oy) = owner_pubkey.xy();
        let mode_base = pallas::Base::from(mode as u64);
        let blind_base = bulla_blind.inner();

        match fee_config {
            Some(config) => {
                let treasury = pallas::Base::from(config.treasury_share as u64);
                let endowment = pallas::Base::from(config.endowment_share as u64);
                poseidon_hash([ox, oy, pool_token_id, mode_base, blind_base, treasury, endowment])
            }
            None => {
                poseidon_hash([ox, oy, pool_token_id, mode_base, blind_base])
            }
        }
    }
}

// ============================================================================
// MEMBERSHIP NOTE
// ============================================================================

/// Represents a membership note (time-limited)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Membership {
    /// Membership note (unique identifier)
    pub note: MembershipNote,
    /// DAO-Escrow bulla this membership belongs to
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Member's public key
    pub member_pubkey: PublicKey,
    /// Value/maturity of membership
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
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
        let (mx, my) = member_pubkey.xy();
        poseidon_hash([
            dao_escrow_bulla,
            mx,
            my,
            pallas::Base::from(value),
            token_id,
            pallas::Base::from(expiry),
            blind.inner(),
        ])
    }
}

// ============================================================================
// PARAMETERS (for contract calls)
// ============================================================================

/// Parameters for `DaoEscrow::InitializeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParamsV1 {
    /// The controlling DAO's bulla
    pub dao_bulla: DaoEscrowBulla,
    /// Owner's public key
    pub owner_pubkey: PublicKey,
    /// Endowment token ID
    pub endowment_token_id: pallas::Base,
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
    pub token_id: pallas::Base,
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
    pub total_endowment: u64,
    /// Updated member count
    pub member_count: u64,
    /// Member public key
    pub member_pubkey: PublicKey,
    /// Token ID
    pub token_id: pallas::Base,
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
pub type ClaimId = pallas::Base;

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

/// Parameters for proposing an endowment withdrawal (claim)
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
    pub total_endowment: u64,
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
    pub total_treasury: u64,
}
