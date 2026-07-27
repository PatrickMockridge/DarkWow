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

//! Data structures for Identity Contract
//!
//! ## Minimal Viable Information (MVI) Design
//!
//! The key principle: **reveal less than you think you need**.
//!
//! Example: Verifying someone is a DAO member
//! - BAD: Reveal their wallet address, token balance, voting history
//! - GOOD: ZK proof that "this address holds ≥1 token in this DAO"
//!
//! The credential system allows issuers to make claims about holders,
//! and holders to prove those claims without revealing more.

// ============================================================================
// ATTRIBUTE TYPES
// ============================================================================
//
// We use a tagged format for attributes to allow flexible schemas.
// Each attribute is tagged so verifiers know what they're checking.
// ============================================================================

use dwow_sdk::crypto::{IntentCommitment, IntentNullifier, PublicKey};
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::error::ContractError;
use dwow_sdk::pasta::{group::GroupEncoding, pallas};
use dwow_serial::{SerialDecodable, SerialEncodable};

/// Capability identifier: hash of name + credential requirement
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct CapabilityId(pub pallas::Base);
impl CapabilityId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(b: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(b).into_option().map(Self)
    }
}

/// Capability secret: derived from holder pubkey + capability_id
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct CapabilitySecret(pub pallas::Base);
impl CapabilitySecret {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
}

/// Reputation ID: hash of issuer pubkey + relayer pubkey
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ReputationId(pub pallas::Base);
impl ReputationId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(b: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(b).into_option().map(Self)
    }
}

/// Namespace for identity intents (used with generic intent primitives)
pub const IDENTITY_NAMESPACE: u64 = 0x0001;

/// Supported attribute types for credentials
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum AttributeType {
    /// Boolean attribute (e.g., "is adult", "is citizen")
    Boolean,
    /// Numeric attribute (e.g., "age", "score")
    Numeric,
    /// String attribute (e.g., "country", "role")
    String,
    /// Timestamp attribute (e.g., "expiration")
    Timestamp,
    /// Hash attribute (e.g., "credential hash")
    Hash,
}

/// A single attribute in a credential
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Attribute {
    /// Attribute type
    pub attribute_type: AttributeType,
    /// Attribute name/label
    pub name: Vec<u8>,
    /// Attribute value (encoded based on type)
    pub value: Vec<u8>,
}

/// Credential schema defining required and optional attributes
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CredentialSchema {
    /// Schema name
    pub name: Vec<u8>,
    /// Schema version
    pub version: u32,
    /// Required attributes
    pub required_attributes: Vec<Attribute>,
    /// Optional attributes
    pub optional_attributes: Vec<Attribute>,
}

// ============================================================================
// CREDENTIAL STRUCTURES
// ============================================================================

/// Initialize contract parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParams {
    /// Contract version
    pub version: u32,
}

/// Issue credential parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct IssueCredentialParams {
    /// Issuer's public key
    pub issuer_pub: PublicKey,

    /// Holder's public key (committed)
    pub holder_pub: PublicKey,

    /// Credential schema hash
    pub schema_hash: [u8; 32],

    /// Encrypted attributes (encrypted to holder)
    /// Only holder can decrypt and use
    pub encrypted_attributes: Vec<u8>,

    /// Credential commitment (uses generic PrivateIntent commitment)
    /// commitment = poseidon_hash([9001, owner_x, owner_y, namespace, payload_hash, expiry, nonce, blind])
    pub commitment: IntentCommitment,

    /// Credential nullifier (for non-revocation/consumption)
    /// nullifier = poseidon_hash([9002, owner_secret, namespace, nonce, commitment])
    pub nullifier: IntentNullifier,

    /// Issuance timestamp
    pub issued_at: u64,

    /// Expiration timestamp (0 = never expires)
    pub expires_at: u64,

    /// ZK proof that credential is valid
    pub proof: Vec<u8>,

    /// Fee paid for issuance
    pub fee: u64,
}

/// Revoke credential parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevokeCredentialParams {
    /// Issuer's signature on revocation
    pub issuer_sig: Vec<u8>,

    /// The nullifier of the credential being revoked
    pub nullifier: IntentNullifier,

    /// Reason for revocation (encrypted)
    pub reason: Vec<u8>,

    /// Fee paid for revocation
    pub fee: u64,
}

/// Create claim parameters
/// This is typically done off-chain by the holder
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateClaimParams {
    /// The credential nullifier
    pub nullifier: IntentNullifier,

    /// The claim type (e.g., "age_over_18", "dao_member")
    pub claim_type: Vec<u8>,

    /// Predicate for the claim (e.g., ">= 18", "== 1")
    pub predicate: Vec<u8>,

    /// The attributes being revealed in this claim
    /// (not the actual values, just which ones)
    pub revealed_attributes: Vec<Vec<u8>>,

    /// ZK proof for the claim
    pub proof: Vec<u8>,

    /// Fee paid for claim creation (if on-chain)
    pub fee: u64,
}

/// Create claim parameters (Level 1 - Selective Disclosure)
/// This version includes a public predicate_result output
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateClaimParamsL1 {
    /// The credential nullifier
    pub nullifier: IntentNullifier,

    /// The claim type (e.g., "age_over_18", "dao_member")
    pub claim_type: Vec<u8>,

    /// Predicate for the claim (e.g., ">= 18", "== 1")
    pub predicate: Vec<u8>,

    /// The attributes being revealed in this claim
    /// (not the actual values, just which ones)
    pub revealed_attributes: Vec<Vec<u8>>,

    /// ZK proof for the claim (Level 1 with bounded equation)
    pub proof: Vec<u8>,

    /// Public predicate result (1 if predicate satisfied, 0 otherwise)
    /// This is revealed publicly via the ZK circuit's bounded equation
    pub predicate_result: u8,

    /// Fee paid for claim creation (if on-chain)
    pub fee: u64,
}

/// Verify claim parameters
/// This is typically called by a verifier
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyClaimParams {
    /// The claim to verify
    pub claim: Claim,

    /// The verifier's public key
    pub verifier_pub: PublicKey,

    /// Fee paid for verification
    pub fee: u64,
}

/// A generated claim ready for verification
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Claim {
    /// Credential nullifier (proves credential exists)
    pub nullifier: IntentNullifier,

    /// Issuer's public key (who issued this credential)
    pub issuer_pub: PublicKey,

    /// Claim type identifier
    pub claim_type: [u8; 32],

    /// Predicate result (the minimal info revealed)
    /// e.g., for age: just "true" or "false"
    /// e.g., for DAO member: just "is_member" or "not_member"
    pub predicate_result: Vec<u8>,

    /// Revealed attribute names
    pub revealed_attributes: Vec<Vec<u8>>,

    /// ZK proof
    pub proof: Vec<u8>,

    /// Timestamp when claim was created
    pub created_at: u64,

    /// Claim expiration (if any)
    pub expires_at: u64,
}

/// Stored credential record
#[derive(Debug, Clone)]
pub struct Credential {
    pub nullifier: IntentNullifier,
    pub issuer_pub: PublicKey,
    pub holder_pub: PublicKey,
    pub schema_hash: [u8; 32],
    pub commitment: IntentCommitment,
    pub revoked: bool,
    pub issued_at: u64,
    pub expires_at: u64,
}

impl Credential {
    pub const ENCODED_SIZE: usize = 177;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.extend_from_slice(&self.issuer_pub.to_bytes());
        buf.extend_from_slice(&self.holder_pub.to_bytes());
        buf.extend_from_slice(&self.schema_hash);
        buf.extend_from_slice(&self.commitment.to_bytes());
        buf.push(self.revoked as u8);
        buf.extend_from_slice(&self.issued_at.to_le_bytes());
        buf.extend_from_slice(&self.expires_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Credential: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let nullifier = IntentNullifier::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Credential: invalid nullifier: {}", e)))?;
        let issuer_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Credential: invalid issuer_pub: {}", e)))?;
        let holder_pub = PublicKey::from_bytes(data[64..96].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Credential: invalid holder_pub: {}", e)))?;
        let schema_hash: [u8; 32] = data[96..128].try_into().unwrap();
        let commitment = IntentCommitment::from_bytes(data[128..160].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Credential: invalid commitment: {}", e)))?;
        let revoked = data[160] != 0;
        let issued_at = u64::from_le_bytes(data[161..169].try_into().unwrap());
        let expires_at = u64::from_le_bytes(data[169..177].try_into().unwrap());
        Ok(Credential { nullifier, issuer_pub, holder_pub, schema_hash, commitment, revoked, issued_at, expires_at })
    }
}

/// Trusted issuer record
#[derive(Debug, Clone)]
pub struct Issuer {
    pub pub_key: PublicKey,
    pub name: Vec<u8>,
    pub authorized_schemas: Vec<[u8; 32]>,
    pub trusted: bool,
}

impl Issuer {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 34 + self.name.len() + self.authorized_schemas.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.pub_key.to_bytes());
        buf.push(self.name.len() as u8);
        buf.extend_from_slice(&self.name);
        buf.push(self.authorized_schemas.len() as u8);
        for schema in &self.authorized_schemas {
            buf.extend_from_slice(schema);
        }
        buf.push(self.trusted as u8);
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 34 {
            return Err(ContractError::IoError(format!(
                "Issuer: expected at least 34 bytes, got {}", data.len()
            )));
        }
        let pub_key = PublicKey::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Issuer: invalid pub_key: {}", e)))?;
        let name_len = data[32] as usize;
        if data.len() < 34 + name_len {
            return Err(ContractError::IoError(format!(
                "Issuer: name exceeds data length (need {} + {}), got {}",
                34, name_len, data.len()
            )));
        }
        let name = data[33..33 + name_len].to_vec();
        let schemas_pos = 33 + name_len;
        let schemas_len = data[schemas_pos] as usize;
        let expected = schemas_pos + 1 + schemas_len * 32 + 1;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "Issuer: expected {} bytes, got {}", expected, data.len()
            )));
        }
        let mut authorized_schemas = Vec::with_capacity(schemas_len);
        for i in 0..schemas_len {
            let start = schemas_pos + 1 + i * 32;
            authorized_schemas.push(data[start..start + 32].try_into().unwrap());
        }
        let trusted = data[expected - 1] != 0;
        Ok(Issuer { pub_key, name, authorized_schemas, trusted })
    }
}

// ============================================================================
// O-CAP (OBJECT CAPABILITY) STRUCTURES
// ============================================================================
//
// O-Cap authorization: Prove you have a CAPABILITY without revealing WHO you are.
//
// Instead of ACLs ("Alice has access to X"), we use:
//   Capability: "Prove you have capability for X" → ACCESS GRANTED (identity hidden)
//
// Example: Instead of ACL: alice@company → can_merge_pr
//          O-Cap: prove(role >= senior_engineer) → can_merge_pr
//
// This enables authorization without identity tracking.
//
// ============================================================================

/// A named capability derived from a credential
#[derive(Debug, Clone)]
pub struct Capability {
    pub capability_id: CapabilityId,
    pub name: Vec<u8>,
    pub credential_requirement: CredentialRequirement,
    pub issuer_pub: PublicKey,
    pub max_holders: Option<u64>,
    pub issued_count: u64,
}

impl Capability {
    pub fn encode(&self) -> Vec<u8> {
        let req = &self.credential_requirement;
        let cap = 74 + self.name.len() + req.attribute_name.len();
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.capability_id.to_bytes());
        buf.push(self.name.len() as u8);
        buf.extend_from_slice(&self.name);
        // Inline CredentialRequirement
        buf.extend_from_slice(&req.schema_hash);
        buf.extend_from_slice(&req.issuer_pub.to_bytes());
        buf.extend_from_slice(&req.min_threshold.to_le_bytes());
        buf.push(req.attribute_name.len() as u8);
        buf.extend_from_slice(&req.attribute_name);
        buf.extend_from_slice(&self.issuer_pub.to_bytes());
        buf.push(self.max_holders.is_some() as u8);
        if let Some(mh) = self.max_holders {
            buf.extend_from_slice(&mh.to_le_bytes());
        }
        buf.extend_from_slice(&self.issued_count.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 74 {
            return Err(ContractError::IoError(format!(
                "Capability: expected at least 74 bytes, got {}", data.len()
            )));
        }
        let capability_id = CapabilityId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("Capability: invalid capability_id".into()))?;
        let name_len = data[32] as usize;
        let schema_start = 33 + name_len;
        if data.len() < schema_start + 64 {
            return Err(ContractError::IoError("Capability: data too short for schema".into()));
        }
        let name = data[33..schema_start].to_vec();
        let schema_hash: [u8; 32] = data[schema_start..schema_start + 32].try_into().unwrap();
        let req_issuer_pub = PublicKey::from_bytes(data[schema_start + 32..schema_start + 64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Capability: invalid req issuer_pub: {}", e)))?;
        let min_threshold = u64::from_le_bytes(data[schema_start + 64..schema_start + 72].try_into().unwrap());
        let attr_len = data[schema_start + 72] as usize;
        let attr_start = schema_start + 73;
        if data.len() < attr_start + attr_len + 41 {
            return Err(ContractError::IoError("Capability: data too short for attribute".into()));
        }
        let attribute_name = data[attr_start..attr_start + attr_len].to_vec();
        let credential_requirement = CredentialRequirement {
            schema_hash,
            issuer_pub: req_issuer_pub,
            min_threshold,
            attribute_name,
        };
        let pub_start = attr_start + attr_len;
        let issuer_pub = PublicKey::from_bytes(data[pub_start..pub_start + 32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Capability: invalid issuer_pub: {}", e)))?;
        let has_max = data[pub_start + 32] != 0;
        let (max_holders, issued_pos) = if has_max {
            (Some(u64::from_le_bytes(data[pub_start + 33..pub_start + 41].try_into().unwrap())), pub_start + 41)
        } else {
            (None, pub_start + 33)
        };
        let issued_count = u64::from_le_bytes(data[issued_pos..issued_pos + 8].try_into().unwrap());
        Ok(Capability { capability_id, name, credential_requirement, issuer_pub, max_holders, issued_count })
    }
}

/// What a capability requires to be obtained
///
/// Describes the credential schema and threshold needed to qualify
/// for a capability.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CredentialRequirement {
    /// Schema hash this capability requires
    pub schema_hash: [u8; 32],
    /// Issuer public key (must be from this trusted issuer)
    pub issuer_pub: PublicKey,
    /// Minimum threshold (e.g., role >= senior_engineer means min_threshold >= 5)
    pub min_threshold: u64,
    /// Attribute name to check (e.g., "role", "experience_years")
    pub attribute_name: Vec<u8>,
}

impl CredentialRequirement {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 73 + self.attribute_name.len();
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.schema_hash);
        buf.extend_from_slice(&self.issuer_pub.to_bytes());
        buf.extend_from_slice(&self.min_threshold.to_le_bytes());
        buf.push(self.attribute_name.len() as u8);
        buf.extend_from_slice(&self.attribute_name);
        buf
    }
}

/// A proof of capability presented to verifiers
///
/// This is what holders present to prove they have a capability.
/// The proof reveals only that the holder meets the requirement,
/// not their identity or actual attribute values.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CapabilityProof {
    /// Hash of the capability definition
    pub capability_id: CapabilityId,
    /// Nullifier from the underlying credential (proves credential exists)
    pub nullifier: IntentNullifier,
    /// Public predicate result (1 if satisfied, 0 if not)
    pub predicate_result: u8,
    /// Issuer's public key
    pub issuer_pub: PublicKey,
    /// Schema hash
    pub schema_hash: [u8; 32],
    /// ZK proof of capability satisfaction
    pub proof: Vec<u8>,
    /// Capability secret (proves holder owns this capability)
    pub capability_secret: CapabilitySecret,
    /// Timestamp when proof was created
    pub created_at: u64,
}

/// Parameters for registering a new capability type
#[derive(Debug, Clone)]
pub struct RegisterCapabilityParams {
    /// Capability name (e.g., "can_merge_pr")
    pub name: Vec<u8>,
    /// Credential requirement for this capability
    pub credential_requirement: CredentialRequirement,
    /// Maximum holders (None = unlimited)
    pub max_holders: Option<u64>,
    /// Fee paid for registration
    pub fee: u64,
}

/// Parameters for issuing a capability to a holder
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct IssueCapabilityParams {
    /// Capability ID being issued
    pub capability_id: CapabilityId,
    /// Holder's public key
    pub holder_pub: PublicKey,
    /// Credential nullifier (proves holder has required credential)
    pub credential_nullifier: IntentNullifier,
    /// ZK proof of credential satisfaction
    pub proof: Vec<u8>,
    /// Issuer signature over the capability grant
    pub issuer_sig: Vec<u8>,
    /// Fee paid for issuance
    pub fee: u64,
}

/// Parameters for verifying a capability
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyCapabilityParams {
    /// The capability proof to verify
    pub capability_proof: CapabilityProof,
    /// Verifier's public key (who is requesting verification)
    pub verifier_pub: PublicKey,
    /// Fee paid for verification
    pub fee: u64,
}

/// Parameters for revoking a capability
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevokeCapabilityParams {
    /// Capability ID being revoked
    pub capability_id: CapabilityId,
    /// Holder whose capability is being revoked
    pub holder_pub: PublicKey,
    /// Capability secret (proves holder ownership)
    pub capability_secret: CapabilitySecret,
    /// Issuer or holder signature
    pub signature: Vec<u8>,
    /// Reason for revocation
    pub reason: Vec<u8>,
    /// Fee paid for revocation
    pub fee: u64,
}

/// Stored capability record (who holds what capability)
#[derive(Debug, Clone)]
pub struct StoredCapability {
    pub capability_id: CapabilityId,
    pub holder_pub: PublicKey,
    pub secret: CapabilitySecret,
    pub revoked: bool,
    pub issued_at: u64,
    pub expires_at: u64,
}

impl StoredCapability {
    pub const ENCODED_SIZE: usize = 113;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.capability_id.to_bytes());
        buf.extend_from_slice(&self.holder_pub.to_bytes());
        buf.extend_from_slice(&self.secret.to_bytes());
        buf.push(self.revoked as u8);
        buf.extend_from_slice(&self.issued_at.to_le_bytes());
        buf.extend_from_slice(&self.expires_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "StoredCapability: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let capability_id = CapabilityId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("StoredCapability: invalid capability_id".into()))?;
        let holder_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("StoredCapability: invalid holder_pub: {}", e)))?;
        let secret = CapabilitySecret(Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[64..96].try_into().unwrap()),
        ).ok_or_else(|| ContractError::IoError("StoredCapability: invalid secret".into()))?);
        let revoked = data[96] != 0;
        let issued_at = u64::from_le_bytes(data[97..105].try_into().unwrap());
        let expires_at = u64::from_le_bytes(data[105..113].try_into().unwrap());
        Ok(StoredCapability { capability_id, holder_pub, secret, revoked, issued_at, expires_at })
    }
}

// ============================================================================
// COMPETENCY DAG (DIRECTED ACYCLIC GRAPH) STRUCTURES
// ============================================================================
//
// Competency DAGs allow credentials to chain in a DAG structure where
// multiple paths can lead to the same competency.
//
// Example: "Qualified Developer" can be achieved via:
//   PATH A: High School → Associate's → Bachelor's
//   PATH B: Self-Taught + Industry Certification
//
// Each path is an AND chain of credentials.
// Paths are combined with OR logic (any path passes).
//
// ============================================================================

/// A single credential in a DAG path
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DAGCredential {
    /// Credential nullifier (proves credential exists and is valid)
    pub nullifier: IntentNullifier,
    /// Predicate result for this credential (1 if satisfied, 0 if not)
    pub predicate_result: u8,
    /// Claim type identifier
    pub claim_type: [u8; 32],
}

/// A single path in a competency DAG
///
/// A path is an AND chain of credentials. All credentials in a path
/// must pass for the path to be considered "satisfied".
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CredentialPath {
    /// Credentials in this path (AND chain)
    pub credentials: Vec<DAGCredential>,
    /// Merkle root of this path's structure
    pub path_hash: [u8; 32],
}

/// A competency DAG structure
///
/// Defines multiple paths (OR logic) where any path can be satisfied
/// to achieve the DAG's competency.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CompetencyDAG {
    /// Unique DAG identifier
    pub dag_id: [u8; 32],
    /// DAG name (e.g., "Senior Developer", "Medical License")
    pub name: Vec<u8>,
    /// Multiple paths (OR between them)
    pub paths: Vec<CredentialPath>,
    /// Merkle root of entire DAG structure
    pub dag_root: [u8; 32],
    /// Issuer's public key (who defined this DAG)
    pub issuer_pub: PublicKey,
    /// Creation timestamp
    pub created_at: u64,
    /// Expiration timestamp (0 = never)
    pub expires_at: u64,
}

/// Parameters for creating a DAG claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateClaimDAGParams {
    /// The DAG being claimed
    pub dag_id: [u8; 32],
    /// Which path is being satisfied (index into DAG.paths)
    pub path_index: u32,
    /// Credentials for this claim (one per credential in the path)
    pub credentials: Vec<DAGCredential>,
    /// ZK proof that this path is satisfied
    pub proof: Vec<u8>,
    /// Public predicate result (1 if path satisfied, 0 if not)
    pub predicate_result: u8,
    /// Fee paid for claim creation
    pub fee: u64,
}

/// A DAG claim ready for verification
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DAGClaim {
    /// DAG identifier
    pub dag_id: [u8; 32],
    /// Path that was satisfied
    pub path_index: u32,
    /// Credentials in this claim
    pub credentials: Vec<DAGCredential>,
    /// ZK proof
    pub proof: Vec<u8>,
    /// Public predicate result
    pub predicate_result: u8,
    /// Creation timestamp
    pub created_at: u64,
    /// Expiration (if any)
    pub expires_at: u64,
}

/// Parameters for verifying a DAG claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyDAGClaimParams {
    /// The DAG claim to verify
    pub claim: DAGClaim,
    /// The DAG definition being claimed against
    pub dag: CompetencyDAG,
    /// Verifier's public key
    pub verifier_pub: PublicKey,
    /// Fee paid for verification
    pub fee: u64,
}

// ============================================================================
// MINIMAL VIABLE INFORMATION EXAMPLES
// ============================================================================
//
// These examples show how different claims reveal only the minimum necessary
//
// EXAMPLE 1: Age Verification
// ---------------------------
// Credential contains: { DOB: 1990-01-01, country: "US", id_hash: H(SSN) }
//
// BAD (traditional): Reveal DOB, full name, address
// GOOD (MVI): Prove age >= 18 with ZK
//   Claim: { predicate_result: true }
//   Reveals: Only "user is over 18" - nothing else
//
// EXAMPLE 2: DAO Membership
// -----------------------
// Credential contains: { token_balance: 1000, wallet: 0x..., votes: 15 }
//
// BAD (traditional): Reveal wallet address, exact balance
// GOOD (MVI): Prove membership with balance >= 1
//   Claim: { predicate_result: true, revealed: ["is_member"] }
//   Reveals: Only "user holds >= 1 token" - balance hidden
//
// EXAMPLE 3: Accredited Investor
// ------------------------------
// Credential contains: { income: $250k, net_worth: $1M, accredited: true }
//
// BAD (traditional): Reveal exact income and net worth
// GOOD (MVI): Prove accredited = true only
//   Claim: { predicate_result: true }
//   Reveals: Only "user is accredited" - amounts hidden
//
// EXAMPLE 4: O-Cap Authorization
// -----------------------------
// Credential contains: { role: "senior_engineer", experience: 7 }
//
// ACL (traditional): alice@company → can_merge_pr
// O-Cap: prove(role >= senior_engineer) → can_merge_pr
//
// Reveals: Only "user meets requirements" - identity and role hidden
//
// ============================================================================

// ============================================================================
// UPDATE TYPES (for apply_* functions)
// ============================================================================

/// Initialize update
#[derive(Debug, Clone)]
pub struct InitializeUpdateV1 {
    pub version: u32,
    pub created_at: u64,
}

impl InitializeUpdateV1 {
    pub const ENCODED_SIZE: usize = 12;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "InitializeUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let version = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let created_at = u64::from_le_bytes(data[4..12].try_into().unwrap());
        Ok(InitializeUpdateV1 { version, created_at })
    }
}

/// Issue credential update
#[derive(Debug, Clone)]
pub struct IssueCredentialUpdateV1 {
    pub nullifier: IntentNullifier,
    pub issuer_pub: PublicKey,
    pub holder_pub: PublicKey,
    pub schema_hash: [u8; 32],
    pub commitment: IntentCommitment,
    pub issued_at: u64,
    pub expires_at: u64,
}

impl IssueCredentialUpdateV1 {
    pub const ENCODED_SIZE: usize = 176;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.extend_from_slice(&self.issuer_pub.to_bytes());
        buf.extend_from_slice(&self.holder_pub.to_bytes());
        buf.extend_from_slice(&self.schema_hash);
        buf.extend_from_slice(&self.commitment.to_bytes());
        buf.extend_from_slice(&self.issued_at.to_le_bytes());
        buf.extend_from_slice(&self.expires_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "IssueCredentialUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let nullifier = IntentNullifier::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("IssueCredentialUpdateV1: invalid nullifier: {}", e)))?;
        let issuer_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("IssueCredentialUpdateV1: invalid issuer_pub: {}", e)))?;
        let holder_pub = PublicKey::from_bytes(data[64..96].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("IssueCredentialUpdateV1: invalid holder_pub: {}", e)))?;
        let schema_hash: [u8; 32] = data[96..128].try_into().unwrap();
        let commitment = IntentCommitment::from_bytes(data[128..160].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("IssueCredentialUpdateV1: invalid commitment: {}", e)))?;
        let issued_at = u64::from_le_bytes(data[160..168].try_into().unwrap());
        let expires_at = u64::from_le_bytes(data[168..176].try_into().unwrap());
        Ok(IssueCredentialUpdateV1 { nullifier, issuer_pub, holder_pub, schema_hash, commitment, issued_at, expires_at })
    }
}

/// Revoke credential update
#[derive(Debug, Clone)]
pub struct RevokeCredentialUpdateV1 {
    pub nullifier: IntentNullifier,
    pub reason: Vec<u8>,
    pub revoked: bool,
}

impl RevokeCredentialUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 34 + self.reason.len();
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.push(self.reason.len() as u8);
        buf.extend_from_slice(&self.reason);
        buf.push(self.revoked as u8);
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 34 {
            return Err(ContractError::IoError(format!(
                "RevokeCredentialUpdateV1: expected at least 34 bytes, got {}", data.len()
            )));
        }
        let nullifier = IntentNullifier::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("RevokeCredentialUpdateV1: invalid nullifier: {}", e)))?;
        let reason_len = data[32] as usize;
        if data.len() != 34 + reason_len {
            return Err(ContractError::IoError(format!(
                "RevokeCredentialUpdateV1: expected {} bytes, got {}", 34 + reason_len, data.len()
            )));
        }
        let reason = data[33..33 + reason_len].to_vec();
        let revoked = data[33 + reason_len] != 0;
        Ok(RevokeCredentialUpdateV1 { nullifier, reason, revoked })
    }
}

/// Create claim update
#[derive(Debug, Clone)]
pub struct CreateClaimUpdateV1 {
    pub nullifier: IntentNullifier,
    pub claim_type: Vec<u8>,
    pub created_at: u64,
}

impl CreateClaimUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 41 + self.claim_type.len();
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.push(self.claim_type.len() as u8);
        buf.extend_from_slice(&self.claim_type);
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 41 {
            return Err(ContractError::IoError(format!(
                "CreateClaimUpdateV1: expected at least 41 bytes, got {}", data.len()
            )));
        }
        let nullifier = IntentNullifier::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("CreateClaimUpdateV1: invalid nullifier: {}", e)))?;
        let ct_len = data[32] as usize;
        if data.len() != 41 + ct_len {
            return Err(ContractError::IoError(format!(
                "CreateClaimUpdateV1: expected {} bytes, got {}", 41 + ct_len, data.len()
            )));
        }
        let claim_type = data[33..33 + ct_len].to_vec();
        let created_at = u64::from_le_bytes(data[33 + ct_len..41 + ct_len].try_into().unwrap());
        Ok(CreateClaimUpdateV1 { nullifier, claim_type, created_at })
    }
}

/// Verify claim update
#[derive(Debug, Clone)]
pub struct VerifyClaimUpdateV1 {
    pub nullifier: IntentNullifier,
    pub verified: bool,
}

impl VerifyClaimUpdateV1 {
    pub const ENCODED_SIZE: usize = 33;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.push(self.verified as u8);
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "VerifyClaimUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let nullifier = IntentNullifier::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("VerifyClaimUpdateV1: invalid nullifier: {}", e)))?;
        let verified = data[32] != 0;
        Ok(VerifyClaimUpdateV1 { nullifier, verified })
    }
}

/// Register capability update
#[derive(Debug, Clone)]
pub struct RegisterCapabilityUpdateV1 {
    pub capability_id: CapabilityId,
    pub name: Vec<u8>,
    pub credential_requirement: CredentialRequirement,
    pub max_holders: Option<u64>,
}

impl RegisterCapabilityUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let req = &self.credential_requirement;
        let cap = 74 + self.name.len() + req.attribute_name.len();
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.capability_id.to_bytes());
        buf.push(self.name.len() as u8);
        buf.extend_from_slice(&self.name);
        buf.extend_from_slice(&req.schema_hash);
        buf.extend_from_slice(&req.issuer_pub.to_bytes());
        buf.extend_from_slice(&req.min_threshold.to_le_bytes());
        buf.push(req.attribute_name.len() as u8);
        buf.extend_from_slice(&req.attribute_name);
        buf.push(self.max_holders.is_some() as u8);
        if let Some(mh) = self.max_holders {
            buf.extend_from_slice(&mh.to_le_bytes());
        }
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 74 {
            return Err(ContractError::IoError(format!(
                "RegisterCapabilityUpdateV1: expected at least 74 bytes, got {}", data.len()
            )));
        }
        let capability_id = CapabilityId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("RegisterCapabilityUpdateV1: invalid capability_id".into()))?;
        let name_len = data[32] as usize;
        let schema_start = 33 + name_len;
        if data.len() < schema_start + 72 {
            return Err(ContractError::IoError("RegisterCapabilityUpdateV1: data too short".into()));
        }
        let name = data[33..schema_start].to_vec();
        let schema_hash: [u8; 32] = data[schema_start..schema_start + 32].try_into().unwrap();
        let req_issuer_pub = PublicKey::from_bytes(data[schema_start + 32..schema_start + 64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("RegisterCapabilityUpdateV1: invalid req issuer_pub: {}", e)))?;
        let min_threshold = u64::from_le_bytes(data[schema_start + 64..schema_start + 72].try_into().unwrap());
        let attr_len = data[schema_start + 72] as usize;
        let attr_start = schema_start + 73;
        if data.len() < attr_start + attr_len + 1 {
            return Err(ContractError::IoError("RegisterCapabilityUpdateV1: data too short for attribute".into()));
        }
        let attribute_name = data[attr_start..attr_start + attr_len].to_vec();
        let credential_requirement = CredentialRequirement { schema_hash, issuer_pub: req_issuer_pub, min_threshold, attribute_name };
        let has_max = data[attr_start + attr_len] != 0;
        let max_holders = if has_max {
            let mh_start = attr_start + attr_len + 1;
            Some(u64::from_le_bytes(data[mh_start..mh_start + 8].try_into().unwrap()))
        } else {
            None
        };
        Ok(RegisterCapabilityUpdateV1 { capability_id, name, credential_requirement, max_holders })
    }
}

/// Issue capability update
#[derive(Debug, Clone)]
pub struct IssueCapabilityUpdateV1 {
    pub capability_id: CapabilityId,
    pub holder_pub: PublicKey,
    pub capability_secret: CapabilitySecret,
    pub expires_at: u64,
}

impl IssueCapabilityUpdateV1 {
    pub const ENCODED_SIZE: usize = 104;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.capability_id.to_bytes());
        buf.extend_from_slice(&self.holder_pub.to_bytes());
        buf.extend_from_slice(&self.capability_secret.to_bytes());
        buf.extend_from_slice(&self.expires_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "IssueCapabilityUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let capability_id = CapabilityId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("IssueCapabilityUpdateV1: invalid capability_id".into()))?;
        let holder_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("IssueCapabilityUpdateV1: invalid holder_pub: {}", e)))?;
        let capability_secret = CapabilitySecret(Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[64..96].try_into().unwrap()),
        ).ok_or_else(|| ContractError::IoError("IssueCapabilityUpdateV1: invalid capability_secret".into()))?);
        let expires_at = u64::from_le_bytes(data[96..104].try_into().unwrap());
        Ok(IssueCapabilityUpdateV1 { capability_id, holder_pub, capability_secret, expires_at })
    }
}

/// Verify capability update
#[derive(Debug, Clone)]
pub struct VerifyCapabilityUpdateV1 {
    pub capability_id: CapabilityId,
    pub holder_pub: PublicKey,
    pub verified: bool,
}

impl VerifyCapabilityUpdateV1 {
    pub const ENCODED_SIZE: usize = 65;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.capability_id.to_bytes());
        buf.extend_from_slice(&self.holder_pub.to_bytes());
        buf.push(self.verified as u8);
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "VerifyCapabilityUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let capability_id = CapabilityId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("VerifyCapabilityUpdateV1: invalid capability_id".into()))?;
        let holder_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("VerifyCapabilityUpdateV1: invalid holder_pub: {}", e)))?;
        let verified = data[64] != 0;
        Ok(VerifyCapabilityUpdateV1 { capability_id, holder_pub, verified })
    }
}

/// Revoke capability update
#[derive(Debug, Clone)]
pub struct RevokeCapabilityUpdateV1 {
    pub capability_id: CapabilityId,
    pub holder_pub: PublicKey,
}

impl RevokeCapabilityUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.capability_id.to_bytes());
        buf.extend_from_slice(&self.holder_pub.to_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "RevokeCapabilityUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let capability_id = CapabilityId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("RevokeCapabilityUpdateV1: invalid capability_id".into()))?;
        let holder_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("RevokeCapabilityUpdateV1: invalid holder_pub: {}", e)))?;
        Ok(RevokeCapabilityUpdateV1 { capability_id, holder_pub })
    }
}

/// Create DAG claim update
#[derive(Debug, Clone)]
pub struct CreateClaimDAGUpdateV1 {
    pub dag_id: [u8; 32],
    pub path_index: u32,
    pub predicate_result: u8,
}

impl CreateClaimDAGUpdateV1 {
    pub const ENCODED_SIZE: usize = 37;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.dag_id);
        buf.extend_from_slice(&self.path_index.to_le_bytes());
        buf.push(self.predicate_result);
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "CreateClaimDAGUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let dag_id: [u8; 32] = data[0..32].try_into().unwrap();
        let path_index = u32::from_le_bytes(data[32..36].try_into().unwrap());
        let predicate_result = data[36];
        Ok(CreateClaimDAGUpdateV1 { dag_id, path_index, predicate_result })
    }
}

// ============================================================================
// ISSUER REGISTRATION (Phase 2d hardening)
// ============================================================================

/// Register issuer parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterIssuerParams {
    /// Issuer's public key
    pub issuer_pub: PublicKey,
    /// Issuer name/label
    pub name: Vec<u8>,
    /// Authorized schema hashes (empty = all schemas allowed)
    pub authorized_schemas: Vec<[u8; 32]>,
}

/// Register issuer update
#[derive(Debug, Clone)]
pub struct RegisterIssuerUpdateV1 {
    pub issuer_id: PublicKey,
    pub name: Vec<u8>,
    pub authorized_schemas: Vec<[u8; 32]>,
    pub registered_at: u64,
}

impl RegisterIssuerUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 42 + self.name.len() + self.authorized_schemas.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.issuer_id.to_bytes());
        buf.push(self.name.len() as u8);
        buf.extend_from_slice(&self.name);
        buf.push(self.authorized_schemas.len() as u8);
        for schema in &self.authorized_schemas {
            buf.extend_from_slice(schema);
        }
        buf.extend_from_slice(&self.registered_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 42 {
            return Err(ContractError::IoError(format!(
                "RegisterIssuerUpdateV1: expected at least 42 bytes, got {}", data.len()
            )));
        }
        let issuer_id = PublicKey::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("RegisterIssuerUpdateV1: invalid issuer_id: {}", e)))?;
        let name_len = data[32] as usize;
        if data.len() < 42 + name_len {
            return Err(ContractError::IoError("RegisterIssuerUpdateV1: data too short for name".into()));
        }
        let name = data[33..33 + name_len].to_vec();
        let schemas_pos = 33 + name_len;
        let schemas_len = data[schemas_pos] as usize;
        let expected = schemas_pos + 1 + schemas_len * 32 + 8;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "RegisterIssuerUpdateV1: expected {} bytes, got {}", expected, data.len()
            )));
        }
        let mut authorized_schemas = Vec::with_capacity(schemas_len);
        for i in 0..schemas_len {
            let start = schemas_pos + 1 + i * 32;
            authorized_schemas.push(data[start..start + 32].try_into().unwrap());
        }
        let reg_start = schemas_pos + 1 + schemas_len * 32;
        let registered_at = u64::from_le_bytes(data[reg_start..reg_start + 8].try_into().unwrap());
        Ok(RegisterIssuerUpdateV1 { issuer_id, name, authorized_schemas, registered_at })
    }
}

// ============================================================================
// REPUTATION (Phase 2d hardening)
// ============================================================================

/// Update relayer reputation parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateReputationParams {
    /// Issuer's public key (must be a registered issuer)
    pub issuer_pub: PublicKey,
    /// Relayer's public key
    pub relayer_pub: PublicKey,
    /// Total slash count
    pub slash_count: u64,
    /// Total successful withdrawals
    pub success_count: u64,
    /// Total volume processed
    pub total_volume: u64,
    /// Settlement frequency (blocks between settlements, 0 = unknown)
    pub settlement_frequency: u64,
}

/// Reputation record stored on-chain
#[derive(Debug, Clone)]
pub struct ReputationRecord {
    pub relayer_pub: PublicKey,
    pub issuer_pub: PublicKey,
    pub slash_count: u64,
    pub success_count: u64,
    pub total_volume: u64,
    pub settlement_frequency: u64,
    pub last_updated: u64,
}

impl ReputationRecord {
    pub const ENCODED_SIZE: usize = 104;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.relayer_pub.to_bytes());
        buf.extend_from_slice(&self.issuer_pub.to_bytes());
        buf.extend_from_slice(&self.slash_count.to_le_bytes());
        buf.extend_from_slice(&self.success_count.to_le_bytes());
        buf.extend_from_slice(&self.total_volume.to_le_bytes());
        buf.extend_from_slice(&self.settlement_frequency.to_le_bytes());
        buf.extend_from_slice(&self.last_updated.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "ReputationRecord: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let relayer_pub = PublicKey::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("ReputationRecord: invalid relayer_pub: {}", e)))?;
        let issuer_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("ReputationRecord: invalid issuer_pub: {}", e)))?;
        let slash_count = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let success_count = u64::from_le_bytes(data[72..80].try_into().unwrap());
        let total_volume = u64::from_le_bytes(data[80..88].try_into().unwrap());
        let settlement_frequency = u64::from_le_bytes(data[88..96].try_into().unwrap());
        let last_updated = u64::from_le_bytes(data[96..104].try_into().unwrap());
        Ok(ReputationRecord { relayer_pub, issuer_pub, slash_count, success_count, total_volume, settlement_frequency, last_updated })
    }
}

/// Update reputation update
#[derive(Debug, Clone)]
pub struct UpdateReputationUpdateV1 {
    pub reputation_id: ReputationId,
    pub relayer_pub: PublicKey,
    pub issuer_pub: PublicKey,
    pub slash_count: u64,
    pub success_count: u64,
    pub total_volume: u64,
    pub settlement_frequency: u64,
    pub last_updated: u64,
}

impl UpdateReputationUpdateV1 {
    pub const ENCODED_SIZE: usize = 136;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.reputation_id.to_bytes());
        buf.extend_from_slice(&self.relayer_pub.to_bytes());
        buf.extend_from_slice(&self.issuer_pub.to_bytes());
        buf.extend_from_slice(&self.slash_count.to_le_bytes());
        buf.extend_from_slice(&self.success_count.to_le_bytes());
        buf.extend_from_slice(&self.total_volume.to_le_bytes());
        buf.extend_from_slice(&self.settlement_frequency.to_le_bytes());
        buf.extend_from_slice(&self.last_updated.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "UpdateReputationUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let reputation_id = ReputationId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("UpdateReputationUpdateV1: invalid reputation_id".into()))?;
        let relayer_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("UpdateReputationUpdateV1: invalid relayer_pub: {}", e)))?;
        let issuer_pub = PublicKey::from_bytes(data[64..96].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("UpdateReputationUpdateV1: invalid issuer_pub: {}", e)))?;
        let slash_count = u64::from_le_bytes(data[96..104].try_into().unwrap());
        let success_count = u64::from_le_bytes(data[104..112].try_into().unwrap());
        let total_volume = u64::from_le_bytes(data[112..120].try_into().unwrap());
        let settlement_frequency = u64::from_le_bytes(data[120..128].try_into().unwrap());
        let last_updated = u64::from_le_bytes(data[128..136].try_into().unwrap());
        Ok(UpdateReputationUpdateV1 { reputation_id, relayer_pub, issuer_pub, slash_count, success_count, total_volume, settlement_frequency, last_updated })
    }
}