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

use dwow_sdk::crypto::{pasta_prelude::PrimeField, ContractId, PublicKey};
use dwow_sdk::error::ContractError;
use dwow_sdk::pasta::pallas;

/// Read exactly `N` bytes at `offset` — total: `get` + `try_into`, no index and no unwrap.
///
/// The `decode` functions below validate their buffer length before slicing, which makes each
/// `data[a..b]` *provably* in-bounds. But provable is not free: the bounds check still compiles, and
/// its panic location — `Location { file: &'static str, line: u32 }` — is a string and an integer in
/// the contract artifact's data section, which neither `strip` nor `--release` removes. Copied from
/// native_token's model, which holds the same family.
fn read_field<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N], ContractError> {
    data.get(offset..offset.saturating_add(N))
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| {
            ContractError::IoError(format!(
                "truncated field: need {N} bytes at offset {offset}, buffer has {}",
                data.len()
            ))
        })
}

#[allow(dead_code)]
fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 0)?)).ok_or_else(|| ContractError::IoError("invalid base".into())) }

/// State update for `Deploy::Deploy`
#[derive(Clone, Debug)]
pub struct DeployUpdateV1 {
    /// The `ContractId` to deploy
    pub contract_id: ContractId,
    /// Poseidon hash of the WASM bincode for integrity verification
    pub wasm_hash: pallas::Base,
}

impl dwow_serial::Encodable for DeployUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DeployUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl DeployUpdateV1 {
    /// Fixed canonical byte size: contract_id(32) + wasm_hash(32)
    pub const ENCODED_SIZE: usize = 64;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.contract_id.to_bytes());
        buf.extend_from_slice(&self.wasm_hash.to_repr());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "DeployUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let contract_id = ContractId::from_bytes(read_field::<32>(data, 0)?)
            .map_err(|e| ContractError::IoError(format!("DeployUpdateV1: invalid contract_id: {}", e)))?;
        let wasm_hash = Option::<pallas::Base>::from(
            pallas::Base::from_repr(read_field::<32>(data, 32)?),
        )
        .ok_or_else(|| ContractError::IoError("DeployUpdateV1: invalid wasm_hash".into()))?;
        Ok(DeployUpdateV1 { contract_id, wasm_hash })
    }
}

/// Parameters for `Deploy::Lock`
// ANCHOR: deploy-lock-params
#[derive(Clone, Debug)]
pub struct LockParamsV1 {
    /// Public key used to sign the transaction and derive the `ContractId`
    pub public_key: PublicKey,
}

impl dwow_serial::Encodable for LockParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for LockParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl LockParamsV1 { pub const ENCODED_SIZE: usize = 32; pub fn encode(&self) -> Vec<u8> { self.public_key.to_bytes().to_vec() } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("LockParamsV1: expected 32 bytes, got {}", data.len()))); } Ok(LockParamsV1 { public_key: PublicKey::from_bytes(read_field::<32>(data, 0)?).map_err(|e| ContractError::IoError(format!("LockParamsV1: invalid public_key: {}", e)))? }) } }
// ANCHOR_END: deploy-lock-params

/// State update for `Deploy::Lock`
#[derive(Clone, Debug)]
pub struct LockUpdateV1 {
    /// The `ContractId` to lock
    pub contract_id: ContractId,
}

impl dwow_serial::Encodable for LockUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for LockUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl LockUpdateV1 {
    /// Fixed canonical byte size: contract_id(32)
    pub const ENCODED_SIZE: usize = 32;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.contract_id.to_bytes());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "LockUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let contract_id = ContractId::from_bytes(read_field::<32>(data, 0)?)
            .map_err(|e| ContractError::IoError(format!("LockUpdateV1: invalid contract_id: {}", e)))?;
        Ok(LockUpdateV1 { contract_id })
    }
}