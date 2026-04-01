/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Plain Attestation Contract Model
//!
//! # Privacy Notice
//!
//! This contract uses **partial transparency** - state is public on-chain.
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full details.
//!
//! # ZK vs Native Operations
//!
//! | Operation | Method | Reason |
//! |-----------|--------|--------|
//! | Signature verification | ZK (Schnorr) | Sound, constrainable |
//! | Attestation commitment | ZK (Pedersen) | Privacy-preserving |
//! | Delegation ratio | Native Rust | Needs `base_div` (not in ZK) |
//! | Credential chains | Native Rust | Complex graph traversal |

use darkfi_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use darkfi_sdk::crypto::schnorr::Signature;
use darkfi_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// ATTESTATION STATE
// ============================================================================

/// Attestation status
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum AttestationStatus {
    /// Attestation is valid and active
    Active = 0,
    /// Attestation has been revoked
    Revoked = 1,
    /// Attestation has expired
    Expired = 2,
}

/// Delegation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum DelegationType {
    /// No delegation allowed
    None = 0,
    /// Full delegation (same authority)
    Full = 1,
    /// Restricted delegation (with ratio limit)
    Restricted = 2,
}

// ============================================================================
// ATTESTATION (Plain - all fields visible except credential content)
// ============================================================================

/// An attestation/credential
/// PRIVACY NOTICE: Most fields are PUBLIC in plain version.
/// Actual credential content is NOT stored on-chain (only hash).
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Attestation {
    /// Unique attestation identifier (Poseidon hash)
    pub id: pallas::Base,
    /// Schema identifier (defines credential type)
    pub schema_id: pallas::Base,
    /// Attestor's public key (who made the attestation)
    pub attestor: PublicKey,
    /// Subject's public key (who the attestation is about)
    pub subject: PublicKey,
    /// Hash of credential content (stored off-chain)
    /// PRIVACY NOTICE: Actual content is off-chain, only hash is public
    pub content_hash: pallas::Base,
    /// Delegation type
    pub delegation_type: DelegationType,
    /// Maximum delegation depth allowed
    pub max_depth: u32,
    /// Current delegation depth (0 = no delegation)
    pub current_depth: u32,
    /// Block when attestation was created
    pub created_at_block: u64,
    /// Block when attestation expires
    pub expires_at_block: u64,
    /// Current status
    pub status: AttestationStatus,
    /// Reference to parent attestation (if delegated)
    pub parent_id: Option<pallas::Base>,
    /// Attestor's signature
    pub attestor_signature: Option<Signature>,
}

/// An attestor (entity that can make attestations)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Attestor {
    /// Attestor's public key
    pub public_key: PublicKey,
    /// Attestor's stake for delegation trust
    pub stake_amount: u64,
    /// Maximum delegation ratio (basis points)
    pub max_delegation_ratio: u64,
    /// Schema IDs this attestor is authorized for
    pub authorized_schemas: Vec<pallas::Base>,
    /// Whether attestor is active
    pub is_active: bool,
}

// ============================================================================
// PARAMETERS (Input types for contract calls)
// ============================================================================

/// Parameters for registering an attestor
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterAttestorParamsV1 {
    /// Attestor's public key
    pub attestor: PublicKey,
    /// Stake amount for delegation trust
    pub stake_amount: u64,
    /// Maximum delegation ratio (basis points)
    pub max_delegation_ratio: u64,
    /// Authorized schema IDs
    pub authorized_schemas: Vec<pallas::Base>,
    /// Attestor's signature over registration
    pub signature: Signature,
}

/// Parameters for creating an attestation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateAttestationParamsV1 {
    /// Schema identifier
    pub schema_id: pallas::Base,
    /// Subject's public key
    pub subject: PublicKey,
    /// Hash of credential content
    pub content_hash: pallas::Base,
    /// Delegation type
    pub delegation_type: DelegationType,
    /// Maximum delegation depth
    pub max_depth: u32,
    /// Expiry block
    pub expires_at_block: u64,
    /// Attestor's signature over attestation
    pub signature: Signature,
}

/// Parameters for delegating an attestation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DelegateAttestationParamsV1 {
    /// Original attestation ID
    pub attestation_id: pallas::Base,
    /// New subject's public key
    pub new_subject: PublicKey,
    /// Delegation depth (current + 1)
    pub delegation_depth: u32,
    /// Delegator's signature over delegation
    pub signature: Signature,
}

/// Parameters for revoking an attestation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevokeAttestationParamsV1 {
    /// Attestation ID
    pub attestation_id: pallas::Base,
    /// Reason for revocation (hashed)
    pub revocation_reason_hash: pallas::Base,
    /// Attestor's signature over revocation
    pub signature: Signature,
}

/// Parameters for verifying an attestation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyAttestationParamsV1 {
    /// Attestation ID
    pub attestation_id: pallas::Base,
    /// Expected subject (to verify attestation is about correct person)
    pub expected_subject: PublicKey,
    /// Verifier's signature over verification request
    pub signature: Signature,
}

// ============================================================================
// UPDATE TYPES (Output from process_instruction, input to process_update)
// ============================================================================

/// Update produced by attestor registration
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterAttestorUpdateV1 {
    pub attestor: PublicKey,
    pub stake_amount: u64,
    pub max_delegation_ratio: u64,
}

/// Update produced by attestation creation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateAttestationUpdateV1 {
    pub attestation_id: pallas::Base,
    pub schema_id: pallas::Base,
    pub attestor: PublicKey,
    pub subject: PublicKey,
    pub content_hash: pallas::Base,
    pub delegation_type: DelegationType,
    pub max_depth: u32,
    pub expires_at_block: u64,
    pub created_at_block: u64,
}

/// Update produced by delegation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DelegateAttestationUpdateV1 {
    pub attestation_id: pallas::Base,
    pub new_subject: PublicKey,
    pub delegation_depth: u32,
}

/// Update produced by revocation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevokeAttestationUpdateV1 {
    pub attestation_id: pallas::Base,
    pub revocation_reason_hash: pallas::Base,
}