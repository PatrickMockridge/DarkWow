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

//! Attestation contract data structures

use dwow_sdk::{
    crypto::{pasta_prelude::{FromUniformBytes, PrimeField}, poseidon_hash, PublicKey},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};
use dwow_serial::{SerialDecodable, SerialEncodable};

/// Attestation unique identifier (hash of attestation data)
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct AttestationId(pub pallas::Base);

impl AttestationId {
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(AttestationId)
    }
}

/// Claim unique identifier
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ClaimId(pub pallas::Base);

impl ClaimId {
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(ClaimId)
    }
}

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
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Active),
            1 => Ok(Self::Revoked),
            2 => Ok(Self::Expired),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
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
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Verified),
            2 => Ok(Self::Consumed),
            3 => Ok(Self::Rejected),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
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
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Matches),
            1 => Ok(Self::GreaterOrEqual),
            2 => Ok(Self::LessOrEqual),
            3 => Ok(Self::Contains),
            4 => Ok(Self::Custom),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Core attestation data stored on-chain
#[derive(Debug, Clone)]
pub struct Attestation {
    pub version: u8,
    pub id: AttestationId,
    pub attestor_pub: PublicKey,
    pub attestor_secret: pallas::Base,
    pub claim_type: Predicate,
    pub claim_data: Vec<pallas::Base>,
    pub metadata: Vec<u8>,
    pub state: AttestationState,
    pub created_at: u64,
    pub expires_at: Option<u64>,
}

impl Attestation {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 108 + self.claim_data.len() * 32 + self.metadata.len();
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_bytes());
        b.extend_from_slice(&self.attestor_pub.to_bytes());
        b.extend_from_slice(&self.attestor_secret.to_repr());
        b.push(self.claim_type as u8);
        b.push(self.claim_data.len() as u8);
        for d in &self.claim_data { b.extend_from_slice(&d.to_repr()); }
        b.push(self.metadata.len() as u8);
        b.extend_from_slice(&self.metadata);
        b.push(self.state as u8);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.push(self.expires_at.is_some() as u8);
        if let Some(e) = self.expires_at { b.extend_from_slice(&e.to_le_bytes()); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 108 { return Err(ContractError::IoError(format!("Attestation: expected at least 108 bytes, got {}", data.len()))); }
        let version = data[0];
        let id = AttestationId::from_bytes(data[1..33].try_into().unwrap()).ok_or_else(|| ContractError::IoError("Attestation: invalid id".into()))?;
        let attestor_pub = PublicKey::from_bytes(data[33..65].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Attestation: invalid attestor_pub: {}", e)))?;
        let attestor_secret = Option::<pallas::Base>::from(pallas::Base::from_repr(data[65..97].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Attestation: invalid attestor_secret".into()))?;
        let claim_type = Predicate::try_from(data[97])?;
        let cd_count = data[98] as usize;
        let cd_end = 99 + cd_count * 32;
        if data.len() < cd_end + 1 { return Err(ContractError::IoError("Attestation: data too short for claim_data".into())); }
        let mut claim_data = Vec::with_capacity(cd_count);
        for i in 0..cd_count { claim_data.push(Option::<pallas::Base>::from(pallas::Base::from_repr(data[99 + i*32..99 + (i+1)*32].try_into().unwrap())).ok_or_else(|| ContractError::IoError(format!("Attestation: invalid claim_data[{}]", i)))?); }
        let md_len = data[cd_end] as usize;
        let md_end = cd_end + 1 + md_len;
        if data.len() < md_end + 10 { return Err(ContractError::IoError("Attestation: data too short for metadata+state".into())); }
        let metadata = data[cd_end + 1..md_end].to_vec();
        let state = AttestationState::try_from(data[md_end])?;
        let created_at = u64::from_le_bytes(data[md_end + 1..md_end + 9].try_into().unwrap());
        let has_expiry = data[md_end + 9] != 0;
        let expires_at = if has_expiry { Some(u64::from_le_bytes(data[md_end + 10..md_end + 18].try_into().unwrap())) } else { None };
        Ok(Attestation { version, id, attestor_pub, attestor_secret, claim_type, claim_data, metadata, state, created_at, expires_at })
    }
}

impl Attestation {
    /// Derive the attestation ID from attestation parameters
    pub fn derive_id(
        attestor_pub: PublicKey,
        claim_type: Predicate,
        claim_data: &[pallas::Base],
        attestor_secret: pallas::Base,
    ) -> AttestationId {
        let (ax, ay) = attestor_pub.xy().expect("pk not identity");
        // Fold claim_data into a single Base via iterative hashing
        let data_hash = claim_data.iter().fold(pallas::Base::zero(), |acc, x| {
            poseidon_hash([acc, *x])
        });
        AttestationId(poseidon_hash([
            ax, ay,
            pallas::Base::from(claim_type as u64),
            data_hash,
            attestor_secret,
        ]))
    }
}

/// Core claim data stored on-chain
#[derive(Debug, Clone)]
pub struct Claim {
    pub version: u8,
    pub id: ClaimId,
    pub attestation_id: AttestationId,
    pub claimant_pub: PublicKey,
    pub claimant_secret: pallas::Base,
    pub predicate: Predicate,
    pub evidence_commitment: Vec<u8>,
    pub revealed_result: Vec<u8>,
    pub proof: Vec<u8>,
    pub state: ClaimState,
    pub created_at: u64,
    pub consumed_at: Option<u64>,
}

impl Claim {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 143 + self.evidence_commitment.len() + self.revealed_result.len() + self.proof.len();
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_bytes());
        b.extend_from_slice(&self.attestation_id.to_bytes());
        b.extend_from_slice(&self.claimant_pub.to_bytes());
        b.extend_from_slice(&self.claimant_secret.to_repr());
        b.push(self.predicate as u8);
        b.push(self.evidence_commitment.len() as u8);
        b.extend_from_slice(&self.evidence_commitment);
        b.push(self.revealed_result.len() as u8);
        b.extend_from_slice(&self.revealed_result);
        b.push(self.proof.len() as u8);
        b.extend_from_slice(&self.proof);
        b.push(self.state as u8);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.push(self.consumed_at.is_some() as u8);
        if let Some(c) = self.consumed_at { b.extend_from_slice(&c.to_le_bytes()); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 143 { return Err(ContractError::IoError(format!("Claim: expected at least 143 bytes, got {}", data.len()))); }
        let version = data[0];
        let id = ClaimId::from_bytes(data[1..33].try_into().unwrap()).ok_or_else(|| ContractError::IoError("Claim: invalid id".into()))?;
        let attestation_id = AttestationId::from_bytes(data[33..65].try_into().unwrap()).ok_or_else(|| ContractError::IoError("Claim: invalid attestation_id".into()))?;
        let claimant_pub = PublicKey::from_bytes(data[65..97].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Claim: invalid claimant_pub: {}", e)))?;
        let claimant_secret = Option::<pallas::Base>::from(pallas::Base::from_repr(data[97..129].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Claim: invalid claimant_secret".into()))?;
        let predicate = Predicate::try_from(data[129])?;
        let ec_len = data[130] as usize;
        let ec_end = 131 + ec_len;
        if data.len() < ec_end + 1 { return Err(ContractError::IoError("Claim: data too short for evidence_commitment".into())); }
        let evidence_commitment = data[131..ec_end].to_vec();
        let rr_len = data[ec_end] as usize;
        let rr_end = ec_end + 1 + rr_len;
        if data.len() < rr_end + 1 { return Err(ContractError::IoError("Claim: data too short for revealed_result".into())); }
        let revealed_result = data[ec_end + 1..rr_end].to_vec();
        let pr_len = data[rr_end] as usize;
        let pr_end = rr_end + 1 + pr_len;
        if data.len() < pr_end + 10 { return Err(ContractError::IoError("Claim: data too short for proof+state".into())); }
        let proof = data[rr_end + 1..pr_end].to_vec();
        let state = ClaimState::try_from(data[pr_end])?;
        let created_at = u64::from_le_bytes(data[pr_end + 1..pr_end + 9].try_into().unwrap());
        let has_consumed = data[pr_end + 9] != 0;
        let consumed_at = if has_consumed { Some(u64::from_le_bytes(data[pr_end + 10..pr_end + 18].try_into().unwrap())) } else { None };
        Ok(Claim { version, id, attestation_id, claimant_pub, claimant_secret, predicate, evidence_commitment, revealed_result, proof, state, created_at, consumed_at })
    }
}

impl Claim {
    /// Derive the claim ID from claim parameters
    pub fn derive_id(
        attestation_id: AttestationId,
        claimant_pub: PublicKey,
        predicate: Predicate,
        evidence_commitment: &[u8],
        claimant_secret: pallas::Base,
    ) -> ClaimId {
        let (cx, cy) = claimant_pub.xy().expect("pk not identity");
        // Convert evidence_commitment bytes to a Base via iterative hashing
        let evidence_hash = evidence_commitment
            .chunks(32)
            .fold(pallas::Base::zero(), |acc, chunk| {
                let mut repr = [0u8; 32];
                let len = chunk.len().min(32);
                repr[..len].copy_from_slice(&chunk[..len]);
                let mut wide = [0u8; 64];
                wide[..len].copy_from_slice(&chunk[..len]);
                let chunk_val = pallas::Base::from_uniform_bytes(&wide);
                poseidon_hash([acc, chunk_val])
            });
        ClaimId(poseidon_hash([
            attestation_id.inner(),
            cx, cy,
            pallas::Base::from(predicate as u64),
            evidence_hash,
            claimant_secret,
        ]))
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
    /// Attestor's public key
    pub attestor_pub: PublicKey,
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
#[derive(Debug, Clone)]
pub struct CreateAttestationUpdateV1 {
    pub attestation_id: AttestationId,
    pub attestation: Attestation,
    pub index_key: pallas::Base,
}

impl CreateAttestationUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let inner = self.attestation.encode();
        let mut b = Vec::with_capacity(64 + inner.len());
        b.extend_from_slice(&self.attestation_id.to_bytes());
        b.extend_from_slice(&inner);
        b.extend_from_slice(&self.index_key.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 64 { return Err(ContractError::IoError(format!("CreateAttestationUpdateV1: expected at least 64 bytes, got {}", data.len()))); }
        let attestation_id = AttestationId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("CreateAttestationUpdateV1: invalid attestation_id".into()))?;
        let attestation = Attestation::decode(&data[32..data.len() - 32])?;
        let ik_start = 32 + attestation.encode().len();
        if data.len() != ik_start + 32 { return Err(ContractError::IoError(format!("CreateAttestationUpdateV1: size mismatch, expected {}", ik_start + 32))); }
        let index_key = Option::<pallas::Base>::from(pallas::Base::from_repr(data[ik_start..ik_start+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CreateAttestationUpdateV1: invalid index_key".into()))?;
        Ok(CreateAttestationUpdateV1 { attestation_id, attestation, index_key })
    }
}

/// Parameters for revoking an attestation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevokeAttestationParamsV1 {
    /// Attestation ID to revoke
    pub attestation_id: AttestationId,
    /// Attestor's public key
    pub attestor_pub: PublicKey,
}

/// State update for RevokeAttestationV1
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
    /// Claimant's public key
    pub claimant_pub: PublicKey,
    /// Predicate for this claim
    pub predicate: Predicate,
    /// Commitment to evidence
    pub evidence_commitment: Vec<u8>,
    /// The minimal revealed result
    pub revealed_result: Vec<u8>,
}

/// State update for CreateClaimV1
#[derive(Debug, Clone)]
pub struct CreateClaimUpdateV1 {
    pub claim_id: ClaimId,
    pub claim: Claim,
    pub rate_limit_key: pallas::Base,
    pub current_block: u64,
}

impl CreateClaimUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let inner = self.claim.encode();
        let mut b = Vec::with_capacity(72 + inner.len());
        b.extend_from_slice(&self.claim_id.to_bytes());
        b.extend_from_slice(&inner);
        b.extend_from_slice(&self.rate_limit_key.to_repr());
        b.extend_from_slice(&self.current_block.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 72 { return Err(ContractError::IoError(format!("CreateClaimUpdateV1: expected at least 72 bytes, got {}", data.len()))); }
        let claim_id = ClaimId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("CreateClaimUpdateV1: invalid claim_id".into()))?;
        let claim = Claim::decode(&data[32..data.len() - 40])?;
        let tail_start = 32 + claim.encode().len();
        if data.len() != tail_start + 40 { return Err(ContractError::IoError(format!("CreateClaimUpdateV1: size mismatch, expected {}", tail_start + 40))); }
        let rate_limit_key = Option::<pallas::Base>::from(pallas::Base::from_repr(data[tail_start..tail_start+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CreateClaimUpdateV1: invalid rate_limit_key".into()))?;
        let current_block = u64::from_le_bytes(data[tail_start+32..tail_start+40].try_into().unwrap());
        Ok(CreateClaimUpdateV1 { claim_id, claim, rate_limit_key, current_block })
    }
}

/// Parameters for verifying a claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyClaimParamsV1 {
    /// Claim ID to verify
    pub claim_id: ClaimId,
    /// Attestation ID
    pub attestation_id: AttestationId,
    /// Evidence commitment to verify against attestation data
    pub evidence_commitment: pallas::Base,
    /// Revealed result from ZK proof verification
    pub revealed_result: pallas::Base,
    /// Revocation Merkle root
    pub revocation_root: pallas::Base,
    /// Attestation data (hash of claim_data)
    pub attestation_data: pallas::Base,
}

/// State update for VerifyClaimV1
#[derive(Debug, Clone)]
pub struct VerifyClaimUpdateV1 {
    pub claim_id: ClaimId,
    pub verified: bool,
}

impl VerifyClaimUpdateV1 {
    pub const ENCODED_SIZE: usize = 33;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.claim_id.to_bytes());
        buf.push(self.verified as u8);
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "VerifyClaimUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let claim_id = ClaimId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("VerifyClaimUpdateV1: invalid claim_id".into()))?;
        let verified = data[32] != 0;
        Ok(VerifyClaimUpdateV1 { claim_id, verified })
    }
}

/// Parameters for consuming a claim (prevents replay)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConsumeClaimParamsV1 {
    /// Claim ID to consume
    pub claim_id: ClaimId,
    /// Attestation ID
    pub attestation_id: AttestationId,
    /// Claimant's public key
    pub claimant_pub: PublicKey,
    /// Nullifier to prevent double-consumption
    pub nullifier: pallas::Base,
}

/// State update for ConsumeClaimV1
#[derive(Debug, Clone)]
pub struct ConsumeClaimUpdateV1 {
    /// The consumed claim ID
    pub claim_id: ClaimId,
    /// Block height when claim was consumed
    pub consumed_at: u64,
    /// Nullifier to prevent double-consumption
    pub nullifier: pallas::Base,
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
#[derive(Debug, Clone)]
pub struct ValidateClaimUpdateV1 {
    pub claim_id: ClaimId,
    pub valid: bool,
}

impl ValidateClaimUpdateV1 {
    pub const ENCODED_SIZE: usize = 33;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(33); b.extend_from_slice(&self.claim_id.to_bytes()); b.push(self.valid as u8); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("ValidateClaimUpdateV1: expected 33 bytes, got {}", data.len()))); }
        Ok(ValidateClaimUpdateV1 { claim_id: ClaimId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("ValidateClaimUpdateV1: invalid claim_id".into()))?, valid: data[32] != 0 })
    }
}

/// Parameters for delegating an attestation
#[derive(Debug, Clone)]
pub struct DelegateAttestationParamsV1 {
    /// ZK proof for delegation
    pub proof: Vec<u8>,
    /// Unique delegation ID
    pub delegation_id: pallas::Base,
    /// Parent delegation ID in the chain
    pub parent_id: pallas::Base,
    /// Delegator's public key
    pub delegator_pub: PublicKey,
    /// Delegatee's public key
    pub delegatee_pub: PublicKey,
    /// Type of delegation (0=None, 1=Full, 2=Restricted)
    pub delegation_type: u8,
    /// Maximum allowed delegation ratio (e.g., 10000 = 100%)
    pub max_ratio: u64,
    /// Revocation Merkle root
    pub revocation_root: pallas::Base,
    /// Merkle root of the delegation chain tree
    pub chain_root: pallas::Base,
    /// Current chain depth
    pub chain_depth: u64,
    /// Maximum allowed chain depth
    pub max_depth: u64,
    /// Delegator's stake amount
    pub delegator_stake: u64,
    /// Delegatee's stake amount
    pub delegatee_stake: u64,
}

impl DelegateAttestationParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(267 + self.proof.len());
        b.push(self.proof.len() as u8);
        b.extend_from_slice(&self.proof);
        b.extend_from_slice(&self.delegation_id.to_repr());
        b.extend_from_slice(&self.parent_id.to_repr());
        b.extend_from_slice(&self.delegator_pub.to_bytes());
        b.extend_from_slice(&self.delegatee_pub.to_bytes());
        b.push(self.delegation_type);
        b.extend_from_slice(&self.max_ratio.to_le_bytes());
        b.extend_from_slice(&self.revocation_root.to_repr());
        b.extend_from_slice(&self.chain_root.to_repr());
        b.extend_from_slice(&self.chain_depth.to_le_bytes());
        b.extend_from_slice(&self.max_depth.to_le_bytes());
        b.extend_from_slice(&self.delegator_stake.to_le_bytes());
        b.extend_from_slice(&self.delegatee_stake.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 1 {
            return Err(ContractError::IoError(
                "DelegateAttestationParamsV1: data too short for proof length".into(),
            ));
        }
        let proof_len = data[0] as usize;
        let fixed_start = 1 + proof_len;
        if data.len() < fixed_start + 266 {
            return Err(ContractError::IoError(format!(
                "DelegateAttestationParamsV1: expected at least {} bytes, got {}",
                fixed_start + 266,
                data.len()
            )));
        }
        let proof = data[1..1 + proof_len].to_vec();
        let d = &data[fixed_start..];
        let delegation_id =
            Option::<pallas::Base>::from(pallas::Base::from_repr(d[0..32].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError(
                    "DelegateAttestationParamsV1: invalid delegation_id".into(),
                ))?;
        let parent_id =
            Option::<pallas::Base>::from(pallas::Base::from_repr(d[32..64].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError(
                    "DelegateAttestationParamsV1: invalid parent_id".into(),
                ))?;
        let delegator_pub =
            PublicKey::from_bytes(d[64..96].try_into().unwrap()).map_err(|e| {
                ContractError::IoError(format!(
                    "DelegateAttestationParamsV1: invalid delegator_pub: {}",
                    e
                ))
            })?;
        let delegatee_pub =
            PublicKey::from_bytes(d[96..128].try_into().unwrap()).map_err(|e| {
                ContractError::IoError(format!(
                    "DelegateAttestationParamsV1: invalid delegatee_pub: {}",
                    e
                ))
            })?;
        let delegation_type = d[128];
        let max_ratio = u64::from_le_bytes(d[129..137].try_into().unwrap());
        let revocation_root =
            Option::<pallas::Base>::from(pallas::Base::from_repr(d[137..169].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError(
                    "DelegateAttestationParamsV1: invalid revocation_root".into(),
                ))?;
        let chain_root =
            Option::<pallas::Base>::from(pallas::Base::from_repr(d[169..201].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError(
                    "DelegateAttestationParamsV1: invalid chain_root".into(),
                ))?;
        let chain_depth = u64::from_le_bytes(d[201..209].try_into().unwrap());
        let max_depth = u64::from_le_bytes(d[209..217].try_into().unwrap());
        let delegator_stake = u64::from_le_bytes(d[217..225].try_into().unwrap());
        let delegatee_stake = u64::from_le_bytes(d[225..233].try_into().unwrap());
        Ok(DelegateAttestationParamsV1 {
            proof,
            delegation_id,
            parent_id,
            delegator_pub,
            delegatee_pub,
            delegation_type,
            max_ratio,
            revocation_root,
            chain_root,
            chain_depth,
            max_depth,
            delegator_stake,
            delegatee_stake,
        })
    }
}

/// State update for DelegateAttestationV1
#[derive(Debug, Clone)]
pub struct DelegateAttestationUpdateV1 {
    pub delegation_id: pallas::Base,
    pub success: bool,
    pub delegation_params: DelegateAttestationParamsV1,
}

impl DelegateAttestationUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let params_bytes = self.delegation_params.encode();
        let mut b = Vec::with_capacity(33 + params_bytes.len());
        b.extend_from_slice(&self.delegation_id.to_repr());
        b.push(self.success as u8);
        b.extend_from_slice(&params_bytes);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 33 { return Err(ContractError::IoError(format!("DelegateAttestationUpdateV1: expected at least 33 bytes, got {}", data.len()))); }
        let delegation_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DelegateAttestationUpdateV1: invalid delegation_id".into()))?;
        let success = data[32] != 0;
        let delegation_params = DelegateAttestationParamsV1::decode(&data[33..])?;
        Ok(DelegateAttestationUpdateV1 { delegation_id, success, delegation_params })
    }
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
#[derive(Debug, Clone)]
pub struct CheckNotRevokedUpdateV1 {
    /// Whether the nonce is not revoked
    pub is_not_revoked: bool,
    /// Hash of (nonce, revocation_root) for replay protection
    pub proof_hash: pallas::Base,
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
#[derive(Debug, Clone)]
pub struct VerifyChainUpdateV1 {
    /// Whether chain verification passed
    pub success: bool,
}

/// Parameters for updating a delegation
#[derive(Debug, Clone)]
pub struct UpdateDelegationParamsV1 {
    /// ZK proof for delegation update
    pub proof: Vec<u8>,
    /// Original attestation ID being delegated
    pub original_attestation_id: pallas::Base,
    /// Type of delegation (0=None, 1=Full, 2=Restricted)
    pub delegation_type: u8,
    /// Current depth in the delegation chain (incremented)
    pub current_depth: u64,
    /// Maximum allowed chain depth
    pub max_depth: u64,
    /// Delegator's stake amount (for Restricted type)
    pub delegator_stake: u64,
    /// Delegatee's stake amount (for Restricted type)
    pub delegatee_stake: u64,
    /// Maximum allowed ratio (e.g., 10000 = 100%) (for Restricted type)
    pub max_ratio: u64,
}

impl UpdateDelegationParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(74 + self.proof.len());
        b.push(self.proof.len() as u8);
        b.extend_from_slice(&self.proof);
        b.extend_from_slice(&self.original_attestation_id.to_repr());
        b.push(self.delegation_type);
        b.extend_from_slice(&self.current_depth.to_le_bytes());
        b.extend_from_slice(&self.max_depth.to_le_bytes());
        b.extend_from_slice(&self.delegator_stake.to_le_bytes());
        b.extend_from_slice(&self.delegatee_stake.to_le_bytes());
        b.extend_from_slice(&self.max_ratio.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 1 {
            return Err(ContractError::IoError(
                "UpdateDelegationParamsV1: data too short for proof length".into(),
            ));
        }
        let proof_len = data[0] as usize;
        let fixed_start = 1 + proof_len;
        if data.len() < fixed_start + 73 {
            return Err(ContractError::IoError(format!(
                "UpdateDelegationParamsV1: expected at least {} bytes, got {}",
                fixed_start + 73,
                data.len()
            )));
        }
        let proof = data[1..1 + proof_len].to_vec();
        let d = &data[fixed_start..];
        let original_attestation_id =
            Option::<pallas::Base>::from(pallas::Base::from_repr(d[0..32].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError(
                    "UpdateDelegationParamsV1: invalid original_attestation_id".into(),
                ))?;
        let delegation_type = d[32];
        let current_depth = u64::from_le_bytes(d[33..41].try_into().unwrap());
        let max_depth = u64::from_le_bytes(d[41..49].try_into().unwrap());
        let delegator_stake = u64::from_le_bytes(d[49..57].try_into().unwrap());
        let delegatee_stake = u64::from_le_bytes(d[57..65].try_into().unwrap());
        let max_ratio = u64::from_le_bytes(d[65..73].try_into().unwrap());
        Ok(UpdateDelegationParamsV1 {
            proof,
            original_attestation_id,
            delegation_type,
            current_depth,
            max_depth,
            delegator_stake,
            delegatee_stake,
            max_ratio,
        })
    }
}

/// State update for UpdateDelegationV1
#[derive(Debug, Clone)]
pub struct UpdateDelegationUpdateV1 {
    pub success: bool,
    pub original_attestation_id: pallas::Base,
    pub updated_params: UpdateDelegationParamsV1,
}

impl UpdateDelegationUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let params_bytes = self.updated_params.encode();
        let mut b = Vec::with_capacity(33 + params_bytes.len());
        b.push(self.success as u8);
        b.extend_from_slice(&self.original_attestation_id.to_repr());
        b.extend_from_slice(&params_bytes);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 33 { return Err(ContractError::IoError(format!("UpdateDelegationUpdateV1: expected at least 33 bytes, got {}", data.len()))); }
        let success = data[0] != 0;
        let original_attestation_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UpdateDelegationUpdateV1: invalid original_attestation_id".into()))?;
        let updated_params = UpdateDelegationParamsV1::decode(&data[33..])?;
        Ok(UpdateDelegationUpdateV1 { success, original_attestation_id, updated_params })
    }
}

// ============================================================================
// ATTEST SLASH (Phase 2d hardening)
// ============================================================================

/// Parameters for attesting a relayer slash event
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AttestSlashParamsV1 {
    /// Relayer's public key
    pub relayer_pub: PublicKey,
    /// Amount slashed
    pub slash_amount: u64,
    /// Withdrawal ID that triggered the slash
    pub withdrawal_id: pallas::Base,
    /// Block height when slash occurred
    pub block_height: u64,
}

/// Attestation ID derived from slash event
#[derive(Debug, Clone)]
pub struct AttestSlashUpdateV1 {
    pub attestation_id: AttestationId,
    pub slash_amount: u64,
    pub withdrawal_id: pallas::Base,
    pub block_height: u64,
    /// The full attestation to store
    pub attestation: Attestation,
    /// Serialized index key for lookup
    pub index_key_bytes: Vec<u8>,
    /// Whether this is a newly created attestation
    pub is_new: bool,
}

// ============================================================================
// COMMIT FEE SCHEDULE (Phase 3 hardening)
// ============================================================================

/// Parameters for committing a fee schedule
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CommitFeeScheduleParamsV1 {
    /// Attestor/relayer public key
    pub attestor_pub: PublicKey,
    /// Base fee in basis points
    pub base_fee_bp: u64,
    /// Guaranteed withdrawal premium in basis points
    pub guaranteed_premium_bp: u64,
    /// Maximum supported amount
    pub max_amount: u64,
    /// Minimum supported amount
    pub min_amount: u64,
    /// Metadata (supported tokens, etc.)
    pub metadata: Vec<u8>,
}

/// Update for fee schedule commitment
#[derive(Debug, Clone)]
pub struct CommitFeeScheduleUpdateV1 {
    pub attestation_id: pallas::Base,
    pub base_fee_bp: u64,
    pub guaranteed_premium_bp: u64,
    pub max_amount: u64,
    pub min_amount: u64,
    /// The full attestation to store
    pub attestation: Attestation,
    /// Serialized index key for lookup
    pub index_key_bytes: Vec<u8>,
}

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
// ============================================================================
// Per type-system.md §2.2: bytes round-trip across module boundaries is forbidden.
// Per contract-wasm-type-system.md §3.1: SHALL use explicit encode/decode with
// per-field validating constructors. Per Guardrail 7: LOC is irrelevant.

impl RevokeAttestationUpdateV1 {
    pub const ENCODED_SIZE: usize = 32;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(32); b.extend_from_slice(&self.attestation_id.to_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("RevokeAttestationUpdateV1: expected 32 bytes, got {}", data.len()))); }
        Ok(RevokeAttestationUpdateV1 { attestation_id: AttestationId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("RevokeAttestationUpdateV1: invalid attestation_id".into()))? })
    }
}

impl ExpireAttestationUpdateV1 {
    pub const ENCODED_SIZE: usize = 32;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(32); b.extend_from_slice(&self.attestation_id.to_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("ExpireAttestationUpdateV1: expected 32 bytes, got {}", data.len()))); }
        Ok(ExpireAttestationUpdateV1 { attestation_id: AttestationId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("ExpireAttestationUpdateV1: invalid attestation_id".into()))? })
    }
}

impl ConsumeClaimUpdateV1 {
    pub const ENCODED_SIZE: usize = 72;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(72); b.extend_from_slice(&self.claim_id.to_bytes()); b.extend_from_slice(&self.consumed_at.to_le_bytes()); b.extend_from_slice(&self.nullifier.to_repr()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("ConsumeClaimUpdateV1: expected 72 bytes, got {}", data.len()))); }
        Ok(ConsumeClaimUpdateV1 { claim_id: ClaimId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("ConsumeClaimUpdateV1: invalid claim_id".into()))?, consumed_at: u64::from_le_bytes(data[32..40].try_into().unwrap()), nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[40..72].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ConsumeClaimUpdateV1: invalid nullifier".into()))? })
    }
}

impl CheckNotRevokedUpdateV1 {
    pub const ENCODED_SIZE: usize = 33;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(33); b.push(self.is_not_revoked as u8); b.extend_from_slice(&self.proof_hash.to_repr()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("CheckNotRevokedUpdateV1: expected 33 bytes, got {}", data.len()))); }
        Ok(CheckNotRevokedUpdateV1 { is_not_revoked: data[0] != 0, proof_hash: Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CheckNotRevokedUpdateV1: invalid proof_hash".into()))? })
    }
}

impl VerifyChainUpdateV1 {
    pub const ENCODED_SIZE: usize = 1;
    pub fn encode(&self) -> Vec<u8> { vec![self.success as u8] }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("VerifyChainUpdateV1: expected 1 byte, got {}", data.len()))); }
        Ok(VerifyChainUpdateV1 { success: data[0] != 0 })
    }
}

impl AttestSlashUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let att = self.attestation.encode();
        let mut b = Vec::with_capacity(81 + att.len() + self.index_key_bytes.len());
        b.extend_from_slice(&self.attestation_id.to_bytes());
        b.extend_from_slice(&self.slash_amount.to_le_bytes());
        b.extend_from_slice(&self.withdrawal_id.to_repr());
        b.extend_from_slice(&self.block_height.to_le_bytes());
        b.extend_from_slice(&att);
        b.push(self.index_key_bytes.len() as u8);
        b.extend_from_slice(&self.index_key_bytes);
        b.push(self.is_new as u8);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 81 { return Err(ContractError::IoError(format!("AttestSlashUpdateV1: expected at least 81 bytes, got {}", data.len()))); }
        let attestation_id = AttestationId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("AttestSlashUpdateV1: invalid attestation_id".into()))?;
        let slash_amount = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let withdrawal_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[40..72].try_into().unwrap())).ok_or_else(|| ContractError::IoError("AttestSlashUpdateV1: invalid withdrawal_id".into()))?;
        let block_height = u64::from_le_bytes(data[72..80].try_into().unwrap());
        let attestation = Attestation::decode(&data[80..])?;
        let ikb_pos = 80 + attestation.encode().len();
        if data.len() < ikb_pos + 2 { return Err(ContractError::IoError("AttestSlashUpdateV1: data too short".into())); }
        let ikb_len = data[ikb_pos] as usize;
        if data.len() != ikb_pos + 2 + ikb_len { return Err(ContractError::IoError(format!("AttestSlashUpdateV1: size mismatch"))); }
        let index_key_bytes = data[ikb_pos + 1..ikb_pos + 1 + ikb_len].to_vec();
        let is_new = data[ikb_pos + 1 + ikb_len] != 0;
        Ok(AttestSlashUpdateV1 { attestation_id, slash_amount, withdrawal_id, block_height, attestation, index_key_bytes, is_new })
    }
}

impl CommitFeeScheduleUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let inner = self.attestation.encode();
        let mut b = Vec::with_capacity(65 + inner.len() + self.index_key_bytes.len());
        b.extend_from_slice(&self.attestation_id.to_repr());
        b.extend_from_slice(&self.base_fee_bp.to_le_bytes());
        b.extend_from_slice(&self.guaranteed_premium_bp.to_le_bytes());
        b.extend_from_slice(&self.max_amount.to_le_bytes());
        b.extend_from_slice(&self.min_amount.to_le_bytes());
        b.extend_from_slice(&inner);
        b.push(self.index_key_bytes.len() as u8);
        b.extend_from_slice(&self.index_key_bytes);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 65 { return Err(ContractError::IoError(format!("CommitFeeScheduleUpdateV1: expected at least 65 bytes, got {}", data.len()))); }
        let attestation_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CommitFeeScheduleUpdateV1: invalid attestation_id".into()))?;
        let base_fee_bp = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let guaranteed_premium_bp = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let max_amount = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let min_amount = u64::from_le_bytes(data[56..64].try_into().unwrap());
        let att = Attestation::decode(&data[64..])?;
        let ikb_pos = 64 + att.encode().len();
        if data.len() < ikb_pos + 1 { return Err(ContractError::IoError("CommitFeeScheduleUpdateV1: data too short for index_key".into())); }
        let ikb_len = data[ikb_pos] as usize;
        if data.len() != ikb_pos + 1 + ikb_len { return Err(ContractError::IoError(format!("CommitFeeScheduleUpdateV1: index_key len mismatch, expected {} + 1 + {}", ikb_pos, ikb_len))); }
        let index_key_bytes = data[ikb_pos + 1..].to_vec();
        Ok(CommitFeeScheduleUpdateV1 { attestation_id, base_fee_bp, guaranteed_premium_bp, max_amount, min_amount, attestation: att, index_key_bytes })
    }
}