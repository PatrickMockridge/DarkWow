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

//! DAO-Escrow contract data structures (Simplified MVP)
//!
//! ## Simplified MVP
//!
//! This contract manages an endowment pool governed by a DAO:
//! - DAO-Escrow is identified by an `endowment_bulla` (linked to a DAO bulla)
//! - Members pay premiums into the endowment
//! - Members receive time-limited membership notes
//! - Claims against the endowment are handled by the DAO treasury (not here)

use darkfi_sdk::{
    crypto::{poseidon_hash, BaseBlind, PublicKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// DAO-Escrow unique identifier (hash of parameters)
pub type DaoEscrowBulla = pallas::Base;

/// Membership note identifier
pub type MembershipNote = pallas::Base;

// ============================================================================
// ENDOWMENT CONFIGURATION
// ============================================================================

/// Represents a DAO-Escrow endowment instance
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoEscrow {
    /// Bulla (unique identifier)
    pub bulla: DaoEscrowBulla,
    /// The controlling DAO's bulla
    pub dao_bulla: DaoEscrowBulla,
    /// Owner/creator public key
    pub owner_pubkey: PublicKey,
    /// Token ID held in the endowment
    pub endowment_token_id: pallas::Base,
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
    /// Derive the endowment bulla from parameters
    pub fn derive_bulla(
        dao_bulla: DaoEscrowBulla,
        owner_pubkey: &PublicKey,
        endowment_token_id: pallas::Base,
        bulla_blind: BaseBlind,
    ) -> DaoEscrowBulla {
        let (ox, oy) = owner_pubkey.xy();
        poseidon_hash([
            dao_bulla,
            ox,
            oy,
            endowment_token_id,
            bulla_blind.inner(),
        ])
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
}

/// State update for `DaoEscrow::InitializeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeUpdateV1 {
    /// The created endowment bulla
    pub bulla: DaoEscrowBulla,
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
    pub value_blind: BaseBlind,
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
