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

//! Attestation contract data structures

use darkfi_sdk::{
    crypto::poseidon_hash,
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Attestation unique identifier (hash of attestation data)
pub type AttestationId = pallas::Base;

/// Claim unique identifier
pub type ClaimId = pallas::Base;

/// Represents the state of an attestation
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum AttestationState {
    /// Attestation is active and can be claimed against
    Active = 0,
    /// Attestation has been revoked by attestor
    Revoked = 1,
    /// Attestation has expired (time-based)
    Expired = 2,
}

impl TryFrom<u8> for AttestationState {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Active),
            1 => Ok(Self::Revoked),
            2 => Ok(Self::Expired),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Represents the state of a claim
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum ClaimState {
    /// Claim created but not yet verified
    Pending = 0,
    /// Claim verified valid
    Verified = 1,
    /// Claim consumed (prevents replay)
    Consumed = 2,
    /// Claim verification failed
    Rejected = 3,
}

impl TryFrom<u8> for ClaimState {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Verified),
            2 => Ok(Self::Consumed),
            3 => Ok(Self::Rejected),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Types of predicates that can be verified
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum Predicate {
    /// Evidence must match attestation data exactly
    Matches = 0,
    /// Value >= threshold (for numeric comparisons)
    GreaterOrEqual = 1,
    /// Value <= threshold (for numeric comparisons)
    LessOrEqual = 2,
    /// Data contains a pattern (for string/container checks)
    Contains = 3,
    /// Custom predicate verified via ZK circuit
    Custom = 4,
}

impl TryFrom<u8> for Predicate {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Matches),
            1 => Ok(Self::GreaterOrEqual),
            2 => Ok(Self::LessOrEqual),
            3 => Ok(Self::Contains),
            4 => Ok(Self::Custom),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Core attestation data stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Attestation {
    /// Attestation identifier (commitment)
    pub id: AttestationId,
    /// Attestor's public key x coordinate
    pub attestor_pub_x: pallas::Base,
    /// Attestor's public key y coordinate
    pub attestor_pub_y: pallas::Base,
    /// Secret used for nullifier derivation
    pub attestor_secret: pallas::Base,
    /// Type of claim this attestation represents
    pub claim_type: Predicate,
    /// The commitment/hash data this attestation attesting to
    pub claim_data: Vec<pallas::Base>,
    /// Optional encrypted metadata (visible to specific parties)
    pub metadata: Vec<u8>,
    /// Current state
    pub state: AttestationState,
    /// Block height when attestation was created
    pub created_at: u64,
    /// Block height when attestation expires (None = no expiry)
    pub expires_at: Option<u64>,
}

impl Attestation {
    /// Derive the attestation ID from attestation parameters
    #[allow(dead_code)]
    #[deprecated(note = "Use zk电路 for proper derivation")]
    pub fn derive_id(
        _attestor_pub_x: pallas::Base,
        _attestor_pub_y: pallas::Base,
        _claim_type: Predicate,
        _claim_data: &[pallas::Base],
        _attestor_secret: pallas::Base,
    ) -> AttestationId {
        // This is a placeholder. Proper implementation requires ZK circuits.
        pallas::Base::zero()
    }
}

/// Core claim data stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Claim {
    /// Claim identifier
    pub id: ClaimId,
    /// Attestation this claim is against
    pub attestation_id: AttestationId,
    /// Claimant's public key x coordinate
    pub claimant_pub_x: pallas::Base,
    /// Claimant's public key y coordinate
    pub claimant_pub_y: pallas::Base,
    /// Secret used for nullifier derivation
    pub claimant_secret: pallas::Base,
    /// Predicate for this claim
    pub predicate: Predicate,
    /// Commitment to evidence (H(evidence), not the evidence itself)
    pub evidence_commitment: Vec<u8>,
    /// The minimal revealed result (e.g., true/false for Matches)
    pub revealed_result: Vec<u8>,
    /// ZK proof for predicate verification
    pub proof: Vec<u8>,
    /// Current state
    pub state: ClaimState,
    /// Block height when claim was created
    pub created_at: u64,
    /// Block height when claim was consumed (if consumed)
    pub consumed_at: Option<u64>,
}

impl Claim {
    /// Derive the claim ID from claim parameters
    #[allow(dead_code)]
    #[deprecated(note = "Use zk电路 for proper derivation")]
    pub fn derive_id(
        _attestation_id: AttestationId,
        _claimant_pub_x: pallas::Base,
        _claimant_pub_y: pallas::Base,
        _predicate: Predicate,
        _evidence_commitment: &[u8],
        _claimant_secret: pallas::Base,
    ) -> ClaimId {
        // This is a placeholder. Proper implementation requires ZK circuits.
        pallas::Base::zero()
    }
}

// ============================================================================
// PARAMETERS STRUCTS (for contract calls)
// ============================================================================

/// Parameters for creating an attestation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateAttestationParamsV1 {
    /// ZK proof for attestation creation
    pub proof: Vec<u8>,
    /// Attestation ID
    pub attestation_id: AttestationId,
    /// Attestor's public key x coordinate
    pub attestor_pub_x: pallas::Base,
    /// Attestor's public key y coordinate
    pub attestor_pub_y: pallas::Base,
    /// Type of claim
    pub claim_type: Predicate,
    /// The commitment/hash data
    pub claim_data: Vec<pallas::Base>,
    /// Optional encrypted metadata
    pub metadata: Vec<u8>,
    /// Expiry block (None = no expiry)
    pub expires_at: Option<u64>,
}

/// State update for CreateAttestationV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateAttestationUpdateV1 {
    /// The created attestation ID
    pub attestation_id: AttestationId,
}

/// Parameters for revoking an attestation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevokeAttestationParamsV1 {
    /// Attestation ID to revoke
    pub attestation_id: AttestationId,
    /// Attestor's public key x coordinate
    pub attestor_pub_x: pallas::Base,
    /// Attestor's public key y coordinate
    pub attestor_pub_y: pallas::Base,
}

/// State update for RevokeAttestationV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevokeAttestationUpdateV1 {
    /// The revoked attestation ID
    pub attestation_id: AttestationId,
}

/// Parameters for expiring an attestation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExpireAttestationParamsV1 {
    /// Attestation ID to expire
    pub attestation_id: AttestationId,
}

/// State update for ExpireAttestationV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExpireAttestationUpdateV1 {
    /// The expired attestation ID
    pub attestation_id: AttestationId,
}

/// Parameters for creating a claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateClaimParamsV1 {
    /// ZK proof for claim creation
    pub proof: Vec<u8>,
    /// Claim ID
    pub claim_id: ClaimId,
    /// Attestation ID being claimed against
    pub attestation_id: AttestationId,
    /// Claimant's public key x coordinate
    pub claimant_pub_x: pallas::Base,
    /// Claimant's public key y coordinate
    pub claimant_pub_y: pallas::Base,
    /// Predicate for this claim
    pub predicate: Predicate,
    /// Commitment to evidence
    pub evidence_commitment: Vec<u8>,
    /// The minimal revealed result
    pub revealed_result: Vec<u8>,
}

/// State update for CreateClaimV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateClaimUpdateV1 {
    /// The created claim ID
    pub claim_id: ClaimId,
}

/// Parameters for verifying a claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyClaimParamsV1 {
    /// Claim ID to verify
    pub claim_id: ClaimId,
    /// Attestation ID
    pub attestation_id: AttestationId,
}

/// State update for VerifyClaimV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyClaimUpdateV1 {
    /// The verified claim ID
    pub claim_id: ClaimId,
    /// Whether verification passed
    pub verified: bool,
}

/// Parameters for consuming a claim (prevents replay)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConsumeClaimParamsV1 {
    /// Claim ID to consume
    pub claim_id: ClaimId,
    /// Attestation ID
    pub attestation_id: AttestationId,
    /// Claimant's public key x coordinate
    pub claimant_pub_x: pallas::Base,
    /// Claimant's public key y coordinate
    pub claimant_pub_y: pallas::Base,
    /// Nullifier to prevent double-consumption
    pub nullifier: pallas::Base,
}

/// State update for ConsumeClaimV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConsumeClaimUpdateV1 {
    /// The consumed claim ID
    pub claim_id: ClaimId,
}

/// Parameters for validating a claim (verify without consuming)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ValidateClaimParamsV1 {
    /// Claim ID to validate
    pub claim_id: ClaimId,
    /// Attestation ID
    pub attestation_id: AttestationId,
    /// The evidence to validate against
    pub evidence: Vec<pallas::Base>,
}

/// State update for ValidateClaimV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ValidateClaimUpdateV1 {
    /// The validated claim ID
    pub claim_id: ClaimId,
    /// Whether validation passed
    pub valid: bool,
}

/// Parameters for delegating an attestation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DelegateAttestationParamsV1 {
    /// ZK proof for delegation
    pub proof: Vec<u8>,
    /// Unique delegation ID
    pub delegation_id: pallas::Base,
    /// Delegator's public key x coordinate
    pub delegator_pub_x: pallas::Base,
    /// Delegator's public key y coordinate
    pub delegator_pub_y: pallas::Base,
    /// Delegatee's public key x coordinate
    pub delegatee_pub_x: pallas::Base,
    /// Delegatee's public key y coordinate
    pub delegatee_pub_y: pallas::Base,
    /// Maximum allowed delegation ratio (e.g., 10000 = 100%)
    pub max_ratio: pallas::Base,
    /// Revocation Merkle root
    pub revocation_root: pallas::Base,
    /// Current chain depth
    pub chain_depth: pallas::Base,
    /// Maximum allowed chain depth
    pub max_depth: pallas::Base,
    /// Delegator's stake amount
    pub delegator_stake: pallas::Base,
    /// Delegatee's stake amount
    pub delegatee_stake: pallas::Base,
}

/// State update for DelegateAttestationV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DelegateAttestationUpdateV1 {
    /// The delegation ID
    pub delegation_id: pallas::Base,
    /// Whether delegation was successful
    pub success: bool,
}

/// Parameters for checking not revoked
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CheckNotRevokedParamsV1 {
    /// ZK proof for non-revocation
    pub proof: Vec<u8>,
    /// Revocation Merkle root
    pub revocation_root: pallas::Base,
    /// Nonce being checked
    pub nonce: pallas::Base,
}

/// State update for CheckNotRevokedV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CheckNotRevokedUpdateV1 {
    /// Whether the nonce is not revoked
    pub is_not_revoked: bool,
}

/// Parameters for verifying a delegation chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyChainParamsV1 {
    /// ZK proof for chain verification
    pub proof: Vec<u8>,
    /// Delegation ID being verified
    pub delegation_id: pallas::Base,
    /// Parent delegation ID in the chain
    pub parent_id: pallas::Base,
    /// Merkle root of the delegation chain tree
    pub chain_root: pallas::Base,
    /// Current depth in the delegation chain
    pub current_depth: pallas::Base,
    /// Maximum allowed chain depth
    pub max_depth: pallas::Base,
}

/// State update for VerifyChainV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyChainUpdateV1 {
    /// Whether chain verification passed
    pub success: bool,
}

/// Parameters for updating a delegation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateDelegationParamsV1 {
    /// ZK proof for delegation update
    pub proof: Vec<u8>,
    /// Original attestation ID being delegated
    pub original_attestation_id: pallas::Base,
    /// Type of delegation (0=None, 1=Full, 2=Restricted)
    pub delegation_type: pallas::Base,
    /// Current depth in the delegation chain (incremented)
    pub current_depth: pallas::Base,
    /// Maximum allowed chain depth
    pub max_depth: pallas::Base,
    /// Delegator's stake amount (for Restricted type)
    pub delegator_stake: pallas::Base,
    /// Delegatee's stake amount (for Restricted type)
    pub delegatee_stake: pallas::Base,
    /// Maximum allowed ratio (e.g., 10000 = 100%) (for Restricted type)
    pub max_ratio: pallas::Base,
}

/// State update for UpdateDelegationV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateDelegationUpdateV1 {
    /// Whether the update was successful
    pub success: bool,
}