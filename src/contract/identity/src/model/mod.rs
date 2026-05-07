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

use darkfi_serial::{SerialDecodable, SerialEncodable};
use darkfi_sdk::crypto::{IntentCommitment, IntentNullifier};

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
    pub issuer_pub: [u8; 32],

    /// Holder's public key (committed)
    pub holder_pub: [u8; 32],

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
    pub verifier_pub: [u8; 32],

    /// Fee paid for verification
    pub fee: u64,
}

/// A generated claim ready for verification
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Claim {
    /// Credential nullifier (proves credential exists)
    pub nullifier: IntentNullifier,

    /// Issuer's public key (who issued this credential)
    pub issuer_pub: [u8; 32],

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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Credential {
    /// Credential nullifier
    pub nullifier: IntentNullifier,

    /// Issuer's public key
    pub issuer_pub: [u8; 32],

    /// Holder's public key
    pub holder_pub: [u8; 32],

    /// Schema hash
    pub schema_hash: [u8; 32],

    /// Commitment
    pub commitment: IntentCommitment,

    /// Whether this credential is revoked
    pub revoked: bool,

    /// Issuance timestamp
    pub issued_at: u64,

    /// Expiration timestamp
    pub expires_at: u64,
}

/// Trusted issuer record
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Issuer {
    /// Issuer's public key
    pub pub_key: [u8; 32],

    /// Issuer's name (e.g., "DarkWow DAO")
    pub name: Vec<u8>,

    /// Schema hashes this issuer can issue
    pub authorized_schemas: Vec<[u8; 32]>,

    /// Whether this issuer is currently trusted
    pub trusted: bool,
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
///
/// Capabilities are issued by trusted issuers and prove that the holder
/// meets certain requirements (e.g., role >= senior_engineer).
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Capability {
    /// Unique capability identifier (hash of name + issuer + requirements)
    pub capability_id: [u8; 32],
    /// Capability name (e.g., "can_merge_pr", "can_approve_security_pr")
    pub name: Vec<u8>,
    /// Credential requirements for this capability
    pub credential_requirement: CredentialRequirement,
    /// Issuer's public key (who can issue this capability)
    pub issuer_pub: [u8; 32],
    /// Maximum number of holders (None = unlimited)
    pub max_holders: Option<u64>,
    /// Current number of issued capabilities
    pub issued_count: u64,
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
    pub issuer_pub: [u8; 32],
    /// Minimum threshold (e.g., role >= senior_engineer means min_threshold >= 5)
    pub min_threshold: u64,
    /// Attribute name to check (e.g., "role", "experience_years")
    pub attribute_name: Vec<u8>,
}

/// A proof of capability presented to verifiers
///
/// This is what holders present to prove they have a capability.
/// The proof reveals only that the holder meets the requirement,
/// not their identity or actual attribute values.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CapabilityProof {
    /// Hash of the capability definition
    pub capability_id: [u8; 32],
    /// Nullifier from the underlying credential (proves credential exists)
    pub nullifier: IntentNullifier,
    /// Public predicate result (1 if satisfied, 0 if not)
    pub predicate_result: u8,
    /// Issuer's public key
    pub issuer_pub: [u8; 32],
    /// Schema hash
    pub schema_hash: [u8; 32],
    /// ZK proof of capability satisfaction
    pub proof: Vec<u8>,
    /// Capability secret (proves holder owns this capability)
    pub capability_secret: [u8; 32],
    /// Timestamp when proof was created
    pub created_at: u64,
}

/// Parameters for registering a new capability type
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
    pub capability_id: [u8; 32],
    /// Holder's public key
    pub holder_pub: [u8; 32],
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
    pub verifier_pub: [u8; 32],
    /// Fee paid for verification
    pub fee: u64,
}

/// Parameters for revoking a capability
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevokeCapabilityParams {
    /// Capability ID being revoked
    pub capability_id: [u8; 32],
    /// Holder whose capability is being revoked
    pub holder_pub: [u8; 32],
    /// Capability secret (proves holder ownership)
    pub capability_secret: [u8; 32],
    /// Issuer or holder signature
    pub signature: Vec<u8>,
    /// Reason for revocation
    pub reason: Vec<u8>,
    /// Fee paid for revocation
    pub fee: u64,
}

/// Stored capability record (who holds what capability)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct StoredCapability {
    /// Capability ID
    pub capability_id: [u8; 32],
    /// Holder's public key
    pub holder_pub: [u8; 32],
    /// Capability secret (proves ownership)
    pub secret: [u8; 32],
    /// Whether this capability is revoked
    pub revoked: bool,
    /// Issuance timestamp
    pub issued_at: u64,
    /// Expiration timestamp (0 = never)
    pub expires_at: u64,
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
    pub issuer_pub: [u8; 32],
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
    pub verifier_pub: [u8; 32],
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeUpdateV1 {
    pub version: u32,
    pub created_at: u64,
}

/// Issue credential update
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct IssueCredentialUpdateV1 {
    pub nullifier: IntentNullifier,
    pub issuer_pub: [u8; 32],
    pub holder_pub: [u8; 32],
    pub schema_hash: [u8; 32],
    pub commitment: IntentCommitment,
    pub issued_at: u64,
    pub expires_at: u64,
}

/// Revoke credential update
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevokeCredentialUpdateV1 {
    pub nullifier: IntentNullifier,
    pub reason: Vec<u8>,
    pub revoked: bool,
}

/// Create claim update
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateClaimUpdateV1 {
    pub nullifier: IntentNullifier,
    pub claim_type: Vec<u8>,
    pub created_at: u64,
}

/// Verify claim update
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyClaimUpdateV1 {
    pub nullifier: IntentNullifier,
    pub verified: bool,
}

/// Register capability update
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterCapabilityUpdateV1 {
    pub capability_id: [u8; 32],
    pub name: Vec<u8>,
    pub credential_requirement: CredentialRequirement,
    pub max_holders: Option<u64>,
}

/// Issue capability update
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct IssueCapabilityUpdateV1 {
    pub capability_id: [u8; 32],
    pub holder_pub: [u8; 32],
    pub capability_secret: [u8; 32],
    pub expires_at: u64,
    pub issuance_key: Vec<u8>,
}

/// Verify capability update
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyCapabilityUpdateV1 {
    pub capability_id: [u8; 32],
    pub holder_pub: [u8; 32],
    pub verified: bool,
}

/// Revoke capability update
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevokeCapabilityUpdateV1 {
    pub capability_id: [u8; 32],
    pub holder_pub: [u8; 32],
    pub issuance_key: Vec<u8>,
}

/// Create DAG claim update
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateClaimDAGUpdateV1 {
    pub dag_id: [u8; 32],
    pub path_index: u32,
    pub predicate_result: u8,
}