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

//! Oracle Contract Data Structures
//!
//! This contract demonstrates the "push model" for oracles in DarkWow.
//! Oracles create attestations for external data that other contracts
//! can then verify and consume.

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, PublicKey},
    error::ContractError,
    pasta::pallas,
};

/// Oracle unique identifier
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OracleId(pub pallas::Base);

impl OracleId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(OracleId)
    }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError("OracleId: expected 32 bytes".into())); }
        Self::from_bytes(data.try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("OracleId: invalid".into()))
    }
}

/// Attestation ID — matches dwow_attestation_contract::model::AttestationId exactly.
/// Both contracts must agree on this type definition.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AttestationId(pub pallas::Base);

impl AttestationId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(AttestationId)
    }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError("AttestationId: expected 32 bytes".into())); }
        Self::from_bytes(data.try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("AttestationId: invalid".into()))
    }
}

/// Represents an oracle data feed
#[derive(Debug, Clone)]
pub struct Oracle {
    pub version: u8,
    /// Oracle identifier
    pub id: OracleId,
    /// Oracle operator's public key
    pub oracle_pub: PublicKey,
    /// Name/description of the data feed
    pub name: String,
    /// Type of data (e.g., "price", "weather", "score")
    pub data_type: String,
    /// Current value (updated by oracle)
    pub value: pallas::Base,
    /// Block when value was last updated
    pub updated_at: u64,
    /// Whether oracle is active
    pub is_active: bool,
}

impl Oracle {
    /// Encode to canonical bytes (ρ-calculus: quote).
    /// Layout: version(1) + id(32) + oracle_pub(32) + name_len(u8) + name + data_type_len(u8) + data_type + value(32) + updated_at(8) + is_active(1)
    pub fn encode(&self) -> Vec<u8> {
        let cap = 1 + 32 + 32 + 1 + self.name.len() + 1 + self.data_type.len() + 32 + 8 + 1;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.version);
        buf.extend_from_slice(&self.id.to_bytes());
        buf.extend_from_slice(&self.oracle_pub.to_bytes());
        buf.push(self.name.len() as u8);
        buf.extend_from_slice(self.name.as_bytes());
        buf.push(self.data_type.len() as u8);
        buf.extend_from_slice(self.data_type.as_bytes());
        buf.extend_from_slice(&self.value.to_repr());
        buf.extend_from_slice(&self.updated_at.to_le_bytes());
        buf.push(self.is_active as u8);
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 1 + 32 + 32 + 1 {
            return Err(ContractError::IoError(format!(
                "Oracle: expected at least 66 bytes, got {}", data.len()
            )));
        }
        let version = data[0];
        let id = OracleId::from_bytes(data[1..33].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("Oracle: invalid id".into()))?;
        let oracle_pub = PublicKey::from_bytes(data[33..65].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Oracle: invalid oracle_pub: {}", e)))?;
        let name_len = data[65] as usize;
        let name_end = 66 + name_len;
        if data.len() < name_end + 1 {
            return Err(ContractError::IoError(format!(
                "Oracle: expected at least {} bytes for name, got {}", name_end + 1, data.len()
            )));
        }
        let name = String::from_utf8(data[66..name_end].to_vec())
            .map_err(|e| ContractError::IoError(format!("Oracle: invalid name: {}", e)))?;
        let dtype_len = data[name_end] as usize;
        let dtype_end = name_end + 1 + dtype_len;
        let remaining = 32 + 8 + 1; // value + updated_at + is_active
        if data.len() != dtype_end + remaining {
            return Err(ContractError::IoError(format!(
                "Oracle: expected {} bytes (dtype_end={}, remaining={}), got {}",
                dtype_end + remaining, dtype_end, remaining, data.len()
            )));
        }
        let data_type = String::from_utf8(data[name_end + 1..dtype_end].to_vec())
            .map_err(|e| ContractError::IoError(format!("Oracle: invalid data_type: {}", e)))?;
        let value = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[dtype_end..dtype_end + 32].try_into().unwrap()),
        )
        .ok_or_else(|| ContractError::IoError("Oracle: invalid value".into()))?;
        let updated_at = u64::from_le_bytes(
            data[dtype_end + 32..dtype_end + 40].try_into().unwrap(),
        );
        let is_active = data[dtype_end + 40] != 0;
        Ok(Oracle {
            version,
            id,
            oracle_pub,
            name,
            data_type,
            value,
            updated_at,
            is_active,
        })
    }
}

/// Parameters for registering a new oracle
#[derive(Debug, Clone)]
pub struct RegisterOracleParamsV1 {
    /// ZK proof for oracle registration
    pub proof: Vec<u8>,
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Oracle operator's public key
    pub oracle_pub: PublicKey,
    /// Name of the data feed
    pub name: String,
    /// Type of data
    pub data_type: String,
}

impl dwow_serial::Encodable for RegisterOracleParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RegisterOracleParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl RegisterOracleParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 2 + self.proof.len() + 32 + 32 + 1 + self.name.len() + 1 + self.data_type.len();
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&(self.proof.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.oracle_id.to_bytes());
        buf.extend_from_slice(&self.oracle_pub.to_bytes());
        buf.push(self.name.len() as u8);
        buf.extend_from_slice(self.name.as_bytes());
        buf.push(self.data_type.len() as u8);
        buf.extend_from_slice(self.data_type.as_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 66 { return Err(ContractError::IoError("RegisterOracleParamsV1: too short".into())); }
        let proof_len = u16::from_le_bytes(data[0..2].try_into().unwrap()) as usize;
        let mut pos = 2 + proof_len;
        if data.len() < pos + 32 + 32 + 1 { return Err(ContractError::IoError("RegisterOracleParamsV1: truncated".into())); }
        let proof = data[2..pos].to_vec();
        let oracle_id = OracleId::from_bytes(data[pos..pos+32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("RegisterOracleParamsV1: invalid oracle_id".into()))?;
        pos += 32;
        let oracle_pub = PublicKey::from_bytes(data[pos..pos+32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("RegisterOracleParamsV1: invalid oracle_pub: {}", e)))?;
        pos += 32;
        let name_len = data[pos] as usize; pos += 1;
        if data.len() < pos + name_len + 1 { return Err(ContractError::IoError("RegisterOracleParamsV1: name truncated".into())); }
        let name = String::from_utf8(data[pos..pos+name_len].to_vec())
            .map_err(|e| ContractError::IoError(format!("RegisterOracleParamsV1: invalid name: {}", e)))?;
        pos += name_len;
        let dtype_len = data[pos] as usize; pos += 1;
        if data.len() != pos + dtype_len { return Err(ContractError::IoError("RegisterOracleParamsV1: data_type truncated".into())); }
        let data_type = String::from_utf8(data[pos..pos+dtype_len].to_vec())
            .map_err(|e| ContractError::IoError(format!("RegisterOracleParamsV1: invalid data_type: {}", e)))?;
        Ok(RegisterOracleParamsV1 { proof, oracle_id, oracle_pub, name, data_type })
    }
}

/// Parameters for pushing a new value
#[derive(Debug, Clone)]
pub struct PushValueParamsV1 {
    /// ZK proof for value push
    pub proof: Vec<u8>,
    /// Oracle ID
    pub oracle_id: OracleId,
    /// New value
    pub value: pallas::Base,
}

impl dwow_serial::Encodable for PushValueParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PushValueParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PushValueParamsV1 {
    pub const ENCODED_SIZE_HINT: usize = 66;
    pub fn encode(&self) -> Vec<u8> {
        let cap = 2 + self.proof.len() + 32 + 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&(self.proof.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.oracle_id.to_bytes());
        buf.extend_from_slice(&self.value.to_repr());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 66 { return Err(ContractError::IoError("PushValueParamsV1: too short".into())); }
        let proof_len = u16::from_le_bytes(data[0..2].try_into().unwrap()) as usize;
        let pos = 2 + proof_len;
        if data.len() != pos + 64 { return Err(ContractError::IoError("PushValueParamsV1: wrong length".into())); }
        let proof = data[2..pos].to_vec();
        let oracle_id = OracleId::from_bytes(data[pos..pos+32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("PushValueParamsV1: invalid oracle_id".into()))?;
        let value = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PushValueParamsV1: invalid value".into()))?;
        Ok(PushValueParamsV1 { proof, oracle_id, value })
    }
}

/// Parameters for creating an attestation for external data
#[derive(Debug, Clone)]
pub struct AttestValueParamsV1 {
    /// ZK proof for attestation
    pub proof: Vec<u8>,
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Attestation ID (to be created)
    pub attestation_id: AttestationId,
    /// Predicate type (0=Matches, 1=GreaterOrEqual, 2=LessOrEqual)
    pub predicate: u8,
    /// Threshold value for comparison predicates
    pub threshold: pallas::Base,
}

impl dwow_serial::Encodable for AttestValueParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for AttestValueParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl AttestValueParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 2 + self.proof.len() + 32 + 32 + 1 + 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&(self.proof.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.oracle_id.to_bytes());
        buf.extend_from_slice(&self.attestation_id.to_bytes());
        buf.push(self.predicate);
        buf.extend_from_slice(&self.threshold.to_repr());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 67 { return Err(ContractError::IoError("AttestValueParamsV1: too short".into())); }
        let proof_len = u16::from_le_bytes(data[0..2].try_into().unwrap()) as usize;
        let pos = 2 + proof_len;
        if data.len() != pos + 97 { return Err(ContractError::IoError("AttestValueParamsV1: wrong length".into())); }
        let proof = data[2..pos].to_vec();
        let oracle_id = OracleId::from_bytes(data[pos..pos+32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("AttestValueParamsV1: invalid oracle_id".into()))?;
        let attestation_id = AttestationId::from_bytes(data[pos+32..pos+64].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("AttestValueParamsV1: invalid attestation_id".into()))?;
        let predicate = data[pos+64];
        let threshold = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+65..pos+97].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("AttestValueParamsV1: invalid threshold".into()))?;
        Ok(AttestValueParamsV1 { proof, oracle_id, attestation_id, predicate, threshold })
    }
}

/// Parameters for pushing a commitment to a data point (private value submission)
#[derive(Debug, Clone)]
pub struct PushValueCommitmentParamsV1 {
    /// ZK proof for commitment push
    pub proof: Vec<u8>,
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Commitment (Poseidon hash of value and nonce)
    pub commitment: pallas::Base,
    /// Merkle root of the data tree (public input)
    pub data_root: pallas::Base,
    /// Position in Merkle tree
    pub pos: pallas::Base,
    /// Sparse Merkle path
    pub path: Vec<pallas::Base>,
}

impl dwow_serial::Encodable for PushValueCommitmentParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PushValueCommitmentParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PushValueCommitmentParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 2 + self.proof.len() + 32 + 32 + 32 + 32 + 1 + self.path.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&(self.proof.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.oracle_id.to_bytes());
        buf.extend_from_slice(&self.commitment.to_repr());
        buf.extend_from_slice(&self.data_root.to_repr());
        buf.extend_from_slice(&self.pos.to_repr());
        buf.push(self.path.len() as u8);
        for p in &self.path { buf.extend_from_slice(&p.to_repr()); }
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 130 { return Err(ContractError::IoError("PushValueCommitmentParamsV1: too short".into())); }
        let proof_len = u16::from_le_bytes(data[0..2].try_into().unwrap()) as usize;
        let mut pos = 2 + proof_len;
        if data.len() < pos + 128 + 1 { return Err(ContractError::IoError("PushValueCommitmentParamsV1: truncated".into())); }
        let proof = data[2..pos].to_vec();
        let oracle_id = OracleId::from_bytes(data[pos..pos+32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("PushValueCommitmentParamsV1: invalid oracle_id".into()))?;
        let commitment = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PushValueCommitmentParamsV1: invalid commitment".into()))?;
        let data_root = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+64..pos+96].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PushValueCommitmentParamsV1: invalid data_root".into()))?;
        let pval = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+96..pos+128].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PushValueCommitmentParamsV1: invalid pos".into()))?;
        pos += 128;
        let path_len = data[pos] as usize; pos += 1;
        if data.len() != pos + path_len * 32 { return Err(ContractError::IoError("PushValueCommitmentParamsV1: path truncated".into())); }
        let mut path = Vec::with_capacity(path_len);
        for i in 0..path_len {
            path.push(Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+i*32..pos+(i+1)*32].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("PushValueCommitmentParamsV1: invalid path element".into()))?);
        }
        Ok(PushValueCommitmentParamsV1 { proof, oracle_id, commitment, data_root, pos: pval, path })
    }
}

/// Parameters for aggregating multiple data points
#[derive(Debug, Clone)]
pub struct AggregateParamsV1 {
    /// ZK proof for aggregation
    pub proof: Vec<u8>,
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Computed weighted average result
    pub result: pallas::Base,
    /// Minimum acceptable result
    pub min_result: pallas::Base,
    /// Maximum acceptable result
    pub max_result: pallas::Base,
}

impl dwow_serial::Encodable for AggregateParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for AggregateParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl AggregateParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 2 + self.proof.len() + 32 + 32 + 32 + 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&(self.proof.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.oracle_id.to_bytes());
        buf.extend_from_slice(&self.result.to_repr());
        buf.extend_from_slice(&self.min_result.to_repr());
        buf.extend_from_slice(&self.max_result.to_repr());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 130 { return Err(ContractError::IoError("AggregateParamsV1: too short".into())); }
        let proof_len = u16::from_le_bytes(data[0..2].try_into().unwrap()) as usize;
        let pos = 2 + proof_len;
        if data.len() != pos + 128 { return Err(ContractError::IoError("AggregateParamsV1: wrong length".into())); }
        let proof = data[2..pos].to_vec();
        let oracle_id = OracleId::from_bytes(data[pos..pos+32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("AggregateParamsV1: invalid oracle_id".into()))?;
        let result = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("AggregateParamsV1: invalid result".into()))?;
        let min_result = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+64..pos+96].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("AggregateParamsV1: invalid min_result".into()))?;
        let max_result = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+96..pos+128].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("AggregateParamsV1: invalid max_result".into()))?;
        Ok(AggregateParamsV1 { proof, oracle_id, result, min_result, max_result })
    }
}

// ============================================================================
// UPDATE TYPES (for process_update)
// ============================================================================

/// Update for RegisterOracleV1
#[derive(Debug, Clone)]
pub struct RegisterOracleUpdateV1 {
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Full Oracle to write (constructed in exec)
    pub oracle: Oracle,
}

impl dwow_serial::Encodable for RegisterOracleUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RegisterOracleUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl RegisterOracleUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let inner = self.oracle.encode();
        let mut buf = Vec::with_capacity(32 + inner.len());
        buf.extend_from_slice(&self.oracle_id.to_bytes());
        buf.extend_from_slice(&inner);
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 {
            return Err(ContractError::IoError(format!(
                "RegisterOracleUpdateV1: expected at least 32 bytes, got {}", data.len()
            )));
        }
        let oracle_id = OracleId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("RegisterOracleUpdateV1: invalid oracle_id".into()))?;
        let oracle = Oracle::decode(&data[32..])?;
        Ok(RegisterOracleUpdateV1 { oracle_id, oracle })
    }
}

/// Update for PushValueV1
#[derive(Debug, Clone)]
pub struct PushValueUpdateV1 {
    /// Oracle ID
    pub oracle_id: OracleId,
    /// New value pushed by oracle
    pub value: pallas::Base,
    /// Block height captured in exec for apply
    pub updated_at: u64,
}

impl dwow_serial::Encodable for PushValueUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PushValueUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PushValueUpdateV1 {
    pub const ENCODED_SIZE: usize = 72;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.oracle_id.to_bytes());
        buf.extend_from_slice(&self.value.to_repr());
        buf.extend_from_slice(&self.updated_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "PushValueUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let oracle_id = OracleId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("PushValueUpdateV1: invalid oracle_id".into()))?;
        let value = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PushValueUpdateV1: invalid value".into()))?;
        let updated_at = u64::from_le_bytes(data[64..72].try_into().unwrap());
        Ok(PushValueUpdateV1 { oracle_id, value, updated_at })
    }
}

/// Update for AttestValueV1
#[derive(Debug, Clone)]
pub struct AttestValueUpdateV1 {
    pub oracle_id: OracleId,
    pub attestation_id: AttestationId,
}

impl dwow_serial::Encodable for AttestValueUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for AttestValueUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl AttestValueUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.oracle_id.to_bytes());
        buf.extend_from_slice(&self.attestation_id.to_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "AttestValueUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let oracle_id = OracleId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("AttestValueUpdateV1: invalid oracle_id".into()))?;
        let attestation_id = AttestationId::from_bytes(data[32..64].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("AttestValueUpdateV1: invalid attestation_id".into()))?;
        Ok(AttestValueUpdateV1 { oracle_id, attestation_id })
    }
}

/// Update for PushValueCommitmentV1
#[derive(Debug, Clone)]
pub struct PushValueCommitmentUpdateV1 {
    pub oracle_id: OracleId,
    pub commitment: pallas::Base,
}

impl dwow_serial::Encodable for PushValueCommitmentUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PushValueCommitmentUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PushValueCommitmentUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.oracle_id.to_bytes());
        buf.extend_from_slice(&self.commitment.to_repr());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "PushValueCommitmentUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let oracle_id = OracleId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("PushValueCommitmentUpdateV1: invalid oracle_id".into()))?;
        let commitment = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PushValueCommitmentUpdateV1: invalid commitment".into()))?;
        Ok(PushValueCommitmentUpdateV1 { oracle_id, commitment })
    }
}

/// Update for AggregateV1
#[derive(Debug, Clone)]
pub struct AggregateUpdateV1 {
    pub oracle_id: OracleId,
    pub result: pallas::Base,
    pub updated_at: u64,
}

impl dwow_serial::Encodable for AggregateUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for AggregateUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl AggregateUpdateV1 {
    pub const ENCODED_SIZE: usize = 72;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.oracle_id.to_bytes());
        buf.extend_from_slice(&self.result.to_repr());
        buf.extend_from_slice(&self.updated_at.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "AggregateUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let oracle_id = OracleId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("AggregateUpdateV1: invalid oracle_id".into()))?;
        let result = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("AggregateUpdateV1: invalid result".into()))?;
        let updated_at = u64::from_le_bytes(data[64..72].try_into().unwrap());
        Ok(AggregateUpdateV1 { oracle_id, result, updated_at })
    }
}

/// Parameters for `SetOracleActiveV1`
#[derive(Debug, Clone)]
pub struct SetOracleActiveParamsV1 {
    pub oracle_pub: PublicKey,
    pub is_active: bool,
}

impl dwow_serial::Encodable for SetOracleActiveParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for SetOracleActiveParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl SetOracleActiveParamsV1 {
    pub const ENCODED_SIZE: usize = 33;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.oracle_pub.to_bytes());
        buf.push(self.is_active as u8);
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "SetOracleActiveParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let oracle_pub = PublicKey::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("SetOracleActiveParamsV1: invalid oracle_pub: {}", e)))?;
        let is_active = data[32] != 0;
        Ok(SetOracleActiveParamsV1 { oracle_pub, is_active })
    }
}

/// Update for `SetOracleActiveV1`
#[derive(Debug, Clone)]
pub struct SetOracleActiveUpdateV1 {
    pub oracle_id: OracleId,
    pub is_active: bool,
}

impl dwow_serial::Encodable for SetOracleActiveUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for SetOracleActiveUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl SetOracleActiveUpdateV1 {
    pub const ENCODED_SIZE: usize = 33;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.oracle_id.to_bytes());
        buf.push(self.is_active as u8);
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "SetOracleActiveUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let oracle_id = OracleId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("SetOracleActiveUpdateV1: invalid oracle_id".into()))?;
        let is_active = data[32] != 0;
        Ok(SetOracleActiveUpdateV1 { oracle_id, is_active })
    }
}
