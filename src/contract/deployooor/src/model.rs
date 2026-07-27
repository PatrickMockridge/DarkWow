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

use dwow_sdk::crypto::{ContractId, PublicKey};
use dwow_sdk::error::ContractError;
use dwow_sdk::pasta::pallas;

/// State update for `Deploy::Deploy`
#[derive(Clone, Debug)]
pub struct DeployUpdateV1 {
    /// The `ContractId` to deploy
    pub contract_id: ContractId,
    /// Poseidon hash of the WASM bincode for integrity verification
    pub wasm_hash: pallas::Base,
}

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
        let contract_id = ContractId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("DeployUpdateV1: invalid contract_id".into()))?;
        let wasm_hash = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[32..64].try_into().unwrap()),
        )
        .ok_or_else(|| ContractError::IoError("DeployUpdateV1: invalid wasm_hash".into()))?;
        Ok(DeployUpdateV1 { contract_id, wasm_hash })
    }
}

/// Parameters for `Deploy::Lock`
// ANCHOR: deploy-lock-params
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct LockParamsV1 {
    /// Public key used to sign the transaction and derive the `ContractId`
    pub public_key: PublicKey,
}
// ANCHOR_END: deploy-lock-params

/// State update for `Deploy::Lock`
#[derive(Clone, Debug)]
pub struct LockUpdateV1 {
    /// The `ContractId` to lock
    pub contract_id: ContractId,
}

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
        let contract_id = ContractId::from_bytes(data[0..32].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("LockUpdateV1: invalid contract_id".into()))?;
        Ok(LockUpdateV1 { contract_id })
    }
}