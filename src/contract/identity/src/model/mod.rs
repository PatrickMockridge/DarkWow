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
use dwow_sdk::pasta::pallas;

/// Capability identifier: hash of name + credential requirement
#[derive(Debug, Clone, Copy, Eq, PartialEq,)]
pub struct CapabilityId(pub pallas::Base);
impl CapabilityId {
    pub const ENCODED_SIZE: usize = 32;
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(b: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(b).into_option().map(Self)
    }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("CapabilityId: expected 32 bytes, got {}", data.len()))); }
        Self::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("CapabilityId: invalid".into()))
    }
}

/// Capability secret: derived from holder pubkey + capability_id
#[derive(Debug, Clone, Copy, Eq, PartialEq,)]
pub struct CapabilitySecret(pub pallas::Base);
impl CapabilitySecret {
    pub const ENCODED_SIZE: usize = 32;
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("CapabilitySecret: expected 32 bytes, got {}", data.len()))); } Ok(CapabilitySecret(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CapabilitySecret: invalid".into()))?)) }
}
/// Namespace for identity intents (used with generic intent primitives)
pub const IDENTITY_NAMESPACE: u64 = 0x0001;

/// Supported attribute types for credentials
#[derive(Debug, Clone, PartialEq, Eq,)]
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

impl TryFrom<u8> for AttributeType {
    type Error = ContractError;
    fn try_from(v: u8) -> Result<Self, Self::Error> { match v { 0 => Ok(Self::Boolean), 1 => Ok(Self::Numeric), 2 => Ok(Self::String), 3 => Ok(Self::Timestamp), 4 => Ok(Self::Hash), _ => Err(ContractError::InvalidFunction) } }
}
impl AttributeType { pub fn encode(&self) -> Vec<u8> { vec![self.clone() as u8] } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.is_empty() { return Err(ContractError::IoError("AttributeType: empty".into())); } Self::try_from(data[0]) } }

/// A single attribute in a credential
#[derive(Debug, Clone,)]
pub struct Attribute {
    /// Attribute type
    pub attribute_type: AttributeType,
    /// Attribute name/label
    pub name: Vec<u8>,
    /// Attribute value (encoded based on type)
    pub value: Vec<u8>,
}

impl Attribute { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(3+self.name.len()+self.value.len()); b.extend_from_slice(&self.attribute_type.encode()); b.push(self.name.len() as u8); b.extend_from_slice(&self.name); b.push(self.value.len() as u8); b.extend_from_slice(&self.value); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 4 { return Err(ContractError::IoError("Attribute: too short".into())); } let attribute_type = AttributeType::decode(&data[0..1])?; let name_len = data[1] as usize; let n_end = 2+name_len; if data.len() < n_end+1 { return Err(ContractError::IoError("Attribute: name truncated".into())); } let name = data[2..n_end].to_vec(); let val_len = data[n_end] as usize; if data.len() < n_end+1+val_len { return Err(ContractError::IoError(format!("Attribute: need {} bytes, got {}", n_end+1+val_len, data.len()))); } let value = data[n_end+1..n_end+1+val_len].to_vec(); Ok(Attribute { attribute_type, name, value }) } }

/// Credential schema
#[derive(Debug, Clone,)] pub struct CredentialSchema { pub name: Vec<u8>, pub version: u32, pub required_attributes: Vec<Attribute>, pub optional_attributes: Vec<Attribute> }
impl dwow_serial::Encodable for CredentialSchema { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CredentialSchema { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl CredentialSchema { pub fn encode(&self) -> Vec<u8> { let req_bytes: Vec<Vec<u8>> = self.required_attributes.iter().map(|a| a.encode()).collect(); let opt_bytes: Vec<Vec<u8>> = self.optional_attributes.iter().map(|a| a.encode()).collect(); let cap = 7+self.name.len()+req_bytes.iter().map(|b| b.len()).sum::<usize>()+opt_bytes.iter().map(|b| b.len()).sum::<usize>(); let mut b = Vec::with_capacity(cap); b.push(self.name.len() as u8); b.extend_from_slice(&self.name); b.extend_from_slice(&self.version.to_le_bytes()); b.push(self.required_attributes.len() as u8); for rb in &req_bytes { b.extend_from_slice(rb); } b.push(self.optional_attributes.len() as u8); for ob in &opt_bytes { b.extend_from_slice(ob); } b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 8 { return Err(ContractError::IoError("CredentialSchema: too short".into())); } let name_len = data[0] as usize; let mut pos = 1+name_len; if data.len() < pos+5 { return Err(ContractError::IoError("CredentialSchema: truncated".into())); } let name = data[1..pos].to_vec(); let version = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()); pos += 4; let req_count = data[pos] as usize; pos += 1; let mut required_attributes = Vec::with_capacity(req_count); for _ in 0..req_count { let attr = Attribute::decode(&data[pos..])?; pos += attr.encode().len(); required_attributes.push(attr); } if data.len() < pos+1 { return Err(ContractError::IoError("CredentialSchema: opt count missing".into())); } let opt_count = data[pos] as usize; pos += 1; let mut optional_attributes = Vec::with_capacity(opt_count); for _ in 0..opt_count { let attr = Attribute::decode(&data[pos..])?; pos += attr.encode().len(); optional_attributes.push(attr); } Ok(CredentialSchema { name, version, required_attributes, optional_attributes }) } }

/// Initialize contract parameters
#[derive(Debug, Clone,)] pub struct InitializeParams { pub version: u32 }
impl dwow_serial::Encodable for InitializeParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for InitializeParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl InitializeParams { pub const ENCODED_SIZE: usize = 4; pub fn encode(&self) -> Vec<u8> { self.version.to_le_bytes().to_vec() } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 4 { return Err(ContractError::IoError(format!("InitializeParams: expected 4 bytes, got {}", data.len()))); } Ok(InitializeParams { version: u32::from_le_bytes(data[0..4].try_into().unwrap()) }) } }

// CREDENTIAL STRUCTURES

/// Issue credential parameters
#[derive(Debug, Clone,)]
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

impl dwow_serial::Encodable for IssueCredentialParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for IssueCredentialParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl IssueCredentialParams { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(129+self.encrypted_attributes.len()+self.proof.len()); b.extend_from_slice(&self.issuer_pub.to_bytes()); b.extend_from_slice(&self.holder_pub.to_bytes()); b.extend_from_slice(&self.schema_hash); b.push(self.encrypted_attributes.len() as u8); b.extend_from_slice(&self.encrypted_attributes); b.extend_from_slice(&self.commitment.to_bytes()); b.extend_from_slice(&self.nullifier.to_bytes()); b.extend_from_slice(&self.issued_at.to_le_bytes()); b.extend_from_slice(&self.expires_at.to_le_bytes()); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.fee.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 129 { return Err(ContractError::IoError("IssueCredentialParams: too short".into())); } let issuer_pub = PublicKey::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("IssueCredentialParams: invalid issuer_pub: {}", e)))?; let holder_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("IssueCredentialParams: invalid holder_pub: {}", e)))?; let schema_hash: [u8;32] = data[64..96].try_into().unwrap(); let ea_len = data[96] as usize; let pos = 97+ea_len; if data.len() < pos+64+8+8+1+8 { return Err(ContractError::IoError("IssueCredentialParams: truncated".into())); } let encrypted_attributes = data[97..pos].to_vec(); let commitment = IntentCommitment::from_bytes(data[pos..pos+32].try_into().unwrap()).map_err(|_| ContractError::IoError("IssueCredentialParams: invalid commitment".into()))?; let nullifier = IntentNullifier::from_bytes(data[pos+32..pos+64].try_into().unwrap()).map_err(|_| ContractError::IoError("IssueCredentialParams: invalid nullifier".into()))?; let issued_at = u64::from_le_bytes(data[pos+64..pos+72].try_into().unwrap()); let expires_at = u64::from_le_bytes(data[pos+72..pos+80].try_into().unwrap()); let proof_len = data[pos+80] as usize; let p = pos+81; if data.len() != p+proof_len+8 { return Err(ContractError::IoError(format!("IssueCredentialParams: expected {} bytes, got {}", p+proof_len+8, data.len()))); } let proof = data[p..p+proof_len].to_vec(); let fee = u64::from_le_bytes(data[p+proof_len..p+proof_len+8].try_into().unwrap()); Ok(IssueCredentialParams { issuer_pub, holder_pub, schema_hash, encrypted_attributes, commitment, nullifier, issued_at, expires_at, proof, fee }) } }

/// Revoke credential parameters
#[derive(Debug, Clone,)] pub struct RevokeCredentialParams { pub issuer_sig: Vec<u8>, pub nullifier: IntentNullifier, pub reason: Vec<u8>, pub fee: u64 }
impl RevokeCredentialParams { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(34+self.issuer_sig.len()+self.reason.len()); b.push(self.issuer_sig.len() as u8); b.extend_from_slice(&self.issuer_sig); b.extend_from_slice(&self.nullifier.to_bytes()); b.push(self.reason.len() as u8); b.extend_from_slice(&self.reason); b.extend_from_slice(&self.fee.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 34 { return Err(ContractError::IoError("RevokeCredentialParams: too short".into())); } let sig_len = data[0] as usize; let pos = 1+sig_len; if data.len() < pos+32+1+8 { return Err(ContractError::IoError("RevokeCredentialParams: truncated".into())); } let issuer_sig = data[1..pos].to_vec(); let nullifier = IntentNullifier::from_bytes(data[pos..pos+32].try_into().unwrap()).map_err(|_| ContractError::IoError("RevokeCredentialParams: invalid nullifier".into()))?; let reason_len = data[pos+32] as usize; let r = pos+33; if data.len() != r+reason_len+8 { return Err(ContractError::IoError(format!("RevokeCredentialParams: expected {} bytes, got {}", r+reason_len+8, data.len()))); } let reason = data[r..r+reason_len].to_vec(); let fee = u64::from_le_bytes(data[r+reason_len..r+reason_len+8].try_into().unwrap()); Ok(RevokeCredentialParams { issuer_sig, nullifier, reason, fee }) } }

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
#[derive(Debug, Clone,)]
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

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 73 {
            return Err(ContractError::IoError(format!(
                "CredentialRequirement: expected at least 73 bytes, got {}", data.len()
            )));
        }
        let schema_hash: [u8; 32] = data[0..32].try_into().unwrap();
        let issuer_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("CredentialRequirement: invalid issuer_pub: {}", e)))?;
        let min_threshold = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let attr_len = data[72] as usize;
        if data.len() != 73 + attr_len {
            return Err(ContractError::IoError(format!(
                "CredentialRequirement: expected {} bytes, got {}", 73 + attr_len, data.len()
            )));
        }
        let attribute_name = data[73..73 + attr_len].to_vec();
        Ok(CredentialRequirement { schema_hash, issuer_pub, min_threshold, attribute_name })
    }
}

/// A proof of capability presented to verifiers
///
/// This is what holders present to prove they have a capability.
/// The proof reveals only that the holder meets the requirement,
/// not their identity or actual attribute values.
#[derive(Debug, Clone,)]
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

impl CapabilityProof { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(99+self.proof.len()); b.extend_from_slice(&self.capability_id.encode()); b.extend_from_slice(&self.nullifier.to_bytes()); b.push(self.predicate_result); b.extend_from_slice(&self.issuer_pub.to_bytes()); b.extend_from_slice(&self.schema_hash); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.capability_secret.encode()); b.extend_from_slice(&self.created_at.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 99 { return Err(ContractError::IoError("CapabilityProof: too short".into())); } let capability_id = CapabilityId::decode(&data[0..32])?; let nullifier = IntentNullifier::from_bytes(data[32..64].try_into().unwrap()).map_err(|_| ContractError::IoError("CapabilityProof: invalid nullifier".into()))?; let predicate_result = data[64]; let issuer_pub = PublicKey::from_bytes(data[65..97].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CapabilityProof: invalid issuer_pub: {}", e)))?; let schema_hash: [u8;32] = data[97..129].try_into().unwrap(); let proof_len = data[129] as usize; let p = 130+proof_len; if data.len() < p+40 { return Err(ContractError::IoError("CapabilityProof: truncated".into())); } let proof = data[130..p].to_vec(); let capability_secret = CapabilitySecret::decode(&data[p..p+32])?; let created_at = u64::from_le_bytes(data[p+32..p+40].try_into().unwrap()); Ok(CapabilityProof { capability_id, nullifier, predicate_result, issuer_pub, schema_hash, proof, capability_secret, created_at }) } }

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

impl RegisterCapabilityParams {
    pub fn encode(&self) -> Vec<u8> {
        let cred = self.credential_requirement.encode();
        let cap = 1 + self.name.len() + cred.len() + 1 + 8 + 8;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.name.len() as u8);
        buf.extend_from_slice(&self.name);
        buf.extend_from_slice(&cred);
        buf.push(self.max_holders.is_some() as u8);
        if let Some(mh) = self.max_holders {
            buf.extend_from_slice(&mh.to_le_bytes());
        }
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 10 {
            return Err(ContractError::IoError(format!(
                "RegisterCapabilityParams: expected at least 10 bytes, got {}", data.len()
            )));
        }
        let name_len = data[0] as usize;
        if data.len() < 1 + name_len {
            return Err(ContractError::IoError(format!(
                "RegisterCapabilityParams: name truncated at offset {}", 1 + name_len
            )));
        }
        let name = data[1..1 + name_len].to_vec();
        let req_start = 1 + name_len;
        if data.len() < req_start + 73 {
            return Err(ContractError::IoError(format!(
                "RegisterCapabilityParams: data too short for credential requirement (need 73 at offset {}, have {})",
                req_start, data.len() - req_start
            )));
        }
        let schema_hash: [u8; 32] = data[req_start..req_start + 32].try_into().unwrap();
        let req_issuer_pub = PublicKey::from_bytes(data[req_start + 32..req_start + 64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("RegisterCapabilityParams: invalid issuer_pub: {}", e)))?;
        let min_threshold = u64::from_le_bytes(data[req_start + 64..req_start + 72].try_into().unwrap());
        let attr_len = data[req_start + 72] as usize;
        let attr_start = req_start + 73;
        if data.len() < attr_start + attr_len {
            return Err(ContractError::IoError("RegisterCapabilityParams: data too short for attribute_name".into()));
        }
        let attribute_name = data[attr_start..attr_start + attr_len].to_vec();
        let credential_requirement = CredentialRequirement {
            schema_hash,
            issuer_pub: req_issuer_pub,
            min_threshold,
            attribute_name,
        };
        let pos = attr_start + attr_len;
        if data.len() < pos + 9 {
            return Err(ContractError::IoError(format!(
                "RegisterCapabilityParams: expected at least {} bytes, got {}", pos + 9, data.len()
            )));
        }
        let has_max = data[pos] != 0;
        let max_holders;
        let fee;
        if has_max {
            if data.len() < pos + 17 {
                return Err(ContractError::IoError(format!(
                    "RegisterCapabilityParams: expected at least {} bytes with max_holders, got {}", pos + 17, data.len()
                )));
            }
            max_holders = Some(u64::from_le_bytes(data[pos + 1..pos + 9].try_into().unwrap()));
            fee = u64::from_le_bytes(data[pos + 9..pos + 17].try_into().unwrap());
        } else {
            if data.len() < pos + 9 {
                return Err(ContractError::IoError(format!(
                    "RegisterCapabilityParams: expected at least {} bytes, got {}", pos + 9, data.len()
                )));
            }
            max_holders = None;
            fee = u64::from_le_bytes(data[pos + 1..pos + 9].try_into().unwrap());
        }
        Ok(RegisterCapabilityParams { name, credential_requirement, max_holders, fee })
    }
}

/// Parameters for issuing a capability to a holder
#[derive(Debug, Clone,)] pub struct IssueCapabilityParams { pub capability_id: CapabilityId, pub holder_pub: PublicKey, pub credential_nullifier: IntentNullifier, pub proof: Vec<u8>, pub issuer_sig: Vec<u8>, pub fee: u64 }
impl IssueCapabilityParams { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(98+self.proof.len()+self.issuer_sig.len()); b.extend_from_slice(&self.capability_id.encode()); b.extend_from_slice(&self.holder_pub.to_bytes()); b.extend_from_slice(&self.credential_nullifier.to_bytes()); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b.push(self.issuer_sig.len() as u8); b.extend_from_slice(&self.issuer_sig); b.extend_from_slice(&self.fee.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 98 { return Err(ContractError::IoError("IssueCapabilityParams: too short".into())); } let capability_id = CapabilityId::decode(&data[0..32])?; let holder_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("IssueCapabilityParams: invalid holder_pub: {}", e)))?; let credential_nullifier = IntentNullifier::from_bytes(data[64..96].try_into().unwrap()).map_err(|_| ContractError::IoError("IssueCapabilityParams: invalid credential_nullifier".into()))?; let proof_len = data[96] as usize; let p = 97+proof_len; if data.len() < p+1+8 { return Err(ContractError::IoError("IssueCapabilityParams: truncated".into())); } let proof = data[97..p].to_vec(); let sig_len = data[p] as usize; let s = p+1+sig_len; if data.len() != s+8 { return Err(ContractError::IoError(format!("IssueCapabilityParams: expected {} bytes, got {}", s+8, data.len()))); } let issuer_sig = data[p+1..s].to_vec(); let fee = u64::from_le_bytes(data[s..s+8].try_into().unwrap()); Ok(IssueCapabilityParams { capability_id, holder_pub, credential_nullifier, proof, issuer_sig, fee }) } }

#[derive(Debug, Clone,)] pub struct VerifyCapabilityParams { pub capability_proof: CapabilityProof, pub verifier_pub: PublicKey, pub fee: u64 }
impl VerifyCapabilityParams { pub fn encode(&self) -> Vec<u8> { let proof_bytes = self.capability_proof.encode(); let mut b = Vec::with_capacity(proof_bytes.len()+40); b.extend_from_slice(&proof_bytes); b.extend_from_slice(&self.verifier_pub.to_bytes()); b.extend_from_slice(&self.fee.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 40 { return Err(ContractError::IoError("VerifyCapabilityParams: too short".into())); } let capability_proof = CapabilityProof::decode(data)?; let proof_len = capability_proof.encode().len(); if data.len() != proof_len+40 { return Err(ContractError::IoError(format!("VerifyCapabilityParams: expected {} bytes, got {}", proof_len+40, data.len()))); } let verifier_pub = PublicKey::from_bytes(data[proof_len..proof_len+32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("VerifyCapabilityParams: invalid verifier_pub: {}", e)))?; let fee = u64::from_le_bytes(data[proof_len+32..proof_len+40].try_into().unwrap()); Ok(VerifyCapabilityParams { capability_proof, verifier_pub, fee }) } }

#[derive(Debug, Clone,)] pub struct RevokeCapabilityParams { pub capability_id: CapabilityId, pub holder_pub: PublicKey, pub capability_secret: CapabilitySecret, pub signature: Vec<u8>, pub reason: Vec<u8>, pub fee: u64 }
impl RevokeCapabilityParams { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(98+self.signature.len()+self.reason.len()); b.extend_from_slice(&self.capability_id.encode()); b.extend_from_slice(&self.holder_pub.to_bytes()); b.extend_from_slice(&self.capability_secret.encode()); b.push(self.signature.len() as u8); b.extend_from_slice(&self.signature); b.push(self.reason.len() as u8); b.extend_from_slice(&self.reason); b.extend_from_slice(&self.fee.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 98 { return Err(ContractError::IoError("RevokeCapabilityParams: too short".into())); } let capability_id = CapabilityId::decode(&data[0..32])?; let holder_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("RevokeCapabilityParams: invalid holder_pub: {}", e)))?; let capability_secret = CapabilitySecret::decode(&data[64..96])?; let sig_len = data[96] as usize; let p = 97+sig_len; if data.len() < p+1+8 { return Err(ContractError::IoError("RevokeCapabilityParams: truncated".into())); } let signature = data[97..p].to_vec(); let reason_len = data[p] as usize; let r = p+1+reason_len; if data.len() != r+8 { return Err(ContractError::IoError(format!("RevokeCapabilityParams: expected {} bytes, got {}", r+8, data.len()))); } let reason = data[p+1..r].to_vec(); let fee = u64::from_le_bytes(data[r..r+8].try_into().unwrap()); Ok(RevokeCapabilityParams { capability_id, holder_pub, capability_secret, signature, reason, fee }) } }

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

impl dwow_serial::Encodable for InitializeUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for InitializeUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
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

impl dwow_serial::Encodable for IssueCredentialUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for IssueCredentialUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
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

/// Revoke credential update — carries the full credential record (revoked=true)
/// so apply can blind-write without a db_get (section separation).
#[derive(Debug, Clone)]
pub struct RevokeCredentialUpdateV1 {
    pub credential: Credential,
}

impl dwow_serial::Encodable for RevokeCredentialUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RevokeCredentialUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl RevokeCredentialUpdateV1 {
    pub fn encode(&self) -> Vec<u8> { self.credential.encode() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { Ok(RevokeCredentialUpdateV1 { credential: Credential::decode(data)? }) }
}

/// Register capability update
#[derive(Debug, Clone)]
pub struct RegisterCapabilityUpdateV1 {
    pub capability_id: CapabilityId,
    pub name: Vec<u8>,
    pub credential_requirement: CredentialRequirement,
    pub max_holders: Option<u64>,
}

impl dwow_serial::Encodable for RegisterCapabilityUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RegisterCapabilityUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
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

/// Issue capability update — carries the full capability record (issued_count
/// incremented) so apply can blind-write without a db_get (section separation).
#[derive(Debug, Clone)]
pub struct IssueCapabilityUpdateV1 {
    pub capability: Capability,
}

impl dwow_serial::Encodable for IssueCapabilityUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for IssueCapabilityUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl IssueCapabilityUpdateV1 {
    pub fn encode(&self) -> Vec<u8> { self.capability.encode() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { Ok(IssueCapabilityUpdateV1 { capability: Capability::decode(data)? }) }
}

/// Verify capability update
#[derive(Debug, Clone)]
pub struct VerifyCapabilityUpdateV1 {
    pub capability_id: CapabilityId,
    pub holder_pub: PublicKey,
    pub verified: bool,
}

impl dwow_serial::Encodable for VerifyCapabilityUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for VerifyCapabilityUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
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

impl dwow_serial::Encodable for RevokeCapabilityUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RevokeCapabilityUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
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

// ============================================================================
// ISSUER REGISTRATION (Phase 2d hardening)
// ============================================================================

/// Register issuer parameters
#[derive(Debug, Clone,)]
pub struct RegisterIssuerParams {
    /// Issuer's public key
    pub issuer_pub: PublicKey,
    /// Issuer name/label
    pub name: Vec<u8>,
    /// Authorized schema hashes (empty = all schemas allowed)
    pub authorized_schemas: Vec<[u8; 32]>,
}

impl RegisterIssuerParams { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(34+self.name.len()+self.authorized_schemas.len()*32); b.extend_from_slice(&self.issuer_pub.to_bytes()); b.push(self.name.len() as u8); b.extend_from_slice(&self.name); b.push(self.authorized_schemas.len() as u8); for s in &self.authorized_schemas { b.extend_from_slice(s); } b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 34 { return Err(ContractError::IoError("RegisterIssuerParams: too short".into())); } let issuer_pub = PublicKey::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("RegisterIssuerParams: invalid issuer_pub: {}", e)))?; let name_len = data[32] as usize; let pos = 33+name_len; if data.len() < pos+1 { return Err(ContractError::IoError("RegisterIssuerParams: name truncated".into())); } let name = data[33..pos].to_vec(); let schema_count = data[pos] as usize; let expected = pos+1+schema_count*32; if data.len() != expected { return Err(ContractError::IoError(format!("RegisterIssuerParams: expected {} bytes, got {}", expected, data.len()))); } let mut authorized_schemas = Vec::with_capacity(schema_count); for i in 0..schema_count { authorized_schemas.push(data[pos+1+i*32..pos+1+(i+1)*32].try_into().unwrap()); } Ok(RegisterIssuerParams { issuer_pub, name, authorized_schemas }) } }

/// Register issuer update
#[derive(Debug, Clone)]
pub struct RegisterIssuerUpdateV1 {
    pub issuer_id: PublicKey,
    pub name: Vec<u8>,
    pub authorized_schemas: Vec<[u8; 32]>,
    pub registered_at: u64,
}

impl dwow_serial::Encodable for RegisterIssuerUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RegisterIssuerUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
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
