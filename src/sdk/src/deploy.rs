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

use dwow_serial::{SerialDecodable, SerialEncodable};

use crate::crypto::PublicKey;

// ANCHOR: deploy-deploy-params
/// Parameters for `Deploy::Deploy`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct DeployParamsV1 {
    /// Webassembly bincode of the smart contract
    pub wasm_bincode: Vec<u8>,
    /// Public key used to sign the transaction and derive the `ContractId`
    pub public_key: PublicKey,
    /// Serialized deployment payload instruction
    pub ix: Vec<u8>,
    /// If true, enforce singleton semantics — reject if another contract
    /// claims the same logical name. For critical ecosystem infrastructure
    /// like Promissory Note replicas.
    pub singleton: bool,
    /// Optional logical name for singleton enforcement check.
    /// When singleton=true and name is set, reject if another contract
    /// with this name already exists.
    pub singleton_name: String,
}
// ANCHOR_END: deploy-deploy-params

/// Contract category for on-chain discovery and filtering.
#[derive(Clone, Debug, PartialEq, SerialEncodable, SerialDecodable)]
pub enum Category {
    Token,
    Stablecoin,
    DEX,
    DAO,
    Gaming,
    Identity,
    Infrastructure,
    Finance,
    Labor,
    Insurance,
    Oracle,
    Bridge,
    Other,
}

/// Pointer to an on-chain attestation. The wallet resolves these by querying
/// the Attestation contract's state. Empty for MVP — populated when DAOs or
/// auditors attest to a contract via `CreateAttestationV1`.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct AttestationRef {
    /// Poseidon hash of the attestation
    pub attestation_id: [u8; 32],
    /// Issuer pubkey — Identity::RegisterIssuer must exist for this key
    pub issuer_pubkey: PublicKey,
    /// Block height when the attestation was created
    pub attested_at: u64,
}

/// Self-declared contract metadata carried in `DeployParamsV1::ix`.
/// Displayed as `[UNVERIFIED]` in wallet UI until attestations from trusted
/// issuers are appended. Only `name`, `symbol`, `category`, `description`,
/// and `public` are populated for MVP. `attestations` is always empty.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ContractMetadata {
    /// Human-readable contract name (e.g. "DarkWow Stablecoin")
    pub name: String,
    /// Optional token symbol (e.g. "DRWUSD")
    pub symbol: Option<String>,
    /// Contract category for discovery and filtering
    pub category: Category,
    /// Optional human-readable description
    pub description: Option<String>,
    /// false = unlisted, hidden from public contract views
    pub public: bool,
    /// Future: attestations from DAOs, auditors, identity-verified issuers.
    /// Empty for MVP.
    pub attestations: Vec<AttestationRef>,
}

impl ContractMetadata {
    /// Serialize into bytes for `DeployParamsV1::ix`.
    pub fn to_ix_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        #[expect(clippy::expect_used, reason = "serialization into Vec<u8> is infallible")]
        dwow_serial::Encodable::encode(self, &mut buf)
            .expect("ContractMetadata serialization is infallible");
        buf
    }

    /// Deserialize from `DeployParamsV1::ix` bytes.
    /// Returns None if the bytes are empty or cannot be decoded.
    pub fn from_ix_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        let mut cursor = std::io::Cursor::new(bytes);
        dwow_serial::Decodable::decode(&mut cursor).ok()
    }
}
