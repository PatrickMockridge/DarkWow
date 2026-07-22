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

//! Generic Prover — wallet.md §6.4.1
//!
//! The capability SDK's manifest-path proof builder. Given a ContractId,
//! an action, the selected capabilities, and Seed (§6.1), it constructs ZK
//! proofs for ANY contract — genesis or user-deployed — from its manifest
//! declarations. No compiled-in per-contract builder is required.
//!
//! # Construction (wallet.md §6.4.1 steps 1-6)
//!
//! 1. Resolve the stored manifest by `ContractId`
//! 2. Find the action + its function; the function's `proof_circuit` names
//!    a `[[circuits]]` entry `(name, namespace)`
//! 3. Load the zkas binary for `(ContractId, namespace, name)` from the
//!    `zkas_binaries` store (embedded for genesis, DeployV1-extracted for
//!    user-deployed)
//! 4. Decode it → ordered witness list (`witnesses: Vec<VarType>`)
//! 5. Bind every witness slot per the manifest's `witness_map`
//! 6. Build the proving key (cacheable) → create the proof, with all
//!    randomness derived from `Seed` (§6.1)
//!
//! # Witness-binding rule
//!
//! | Source | Meaning | Typical VarType |
//! |--------|---------|-----------------|
//! | `note:<field>` | Capability's decrypted note field (`note_schema`) | `Base`, `Uint64` |
//! | `param:<field>` | Action's `[[parameters]]` field | per parameter type |
//! | `secret` | The capability's spending key (AccountManager) | `Base` |
//! | `merkle_path` | Inclusion proof (`capability_proofs`) | `MerklePath` |
//! | `leaf_position` | Capability leaf position | `Uint32` |
//! | `blind` | Fresh blind from `Seed` (§6.1) | `Base`, `Scalar` |
//! | `tx_commitment`, `tx_nonce` | Transaction binding names | `Base` |
//!
//! # Implementation note
//!
//! This module defines the abstract API in the SDK. The concrete
//! implementation lives in the wallet binary (`bin/dww`) which has access
//! to `dwow_core` ZK types (`ZkBinary`, `ProvingKey`, `Proof`). The
//! wallet's `ProverImpl` delegates to this module's binding logic.

use crate::crypto::SecretKey;
use crate::manifest::ContractManifest;
use crate::pasta::pallas;

/// The manifest annotations that guide witness binding.
///
/// Each entry in the `[[circuits]]` table carries a `witness_map` — one entry
/// per witness slot, in the EXACT order of the circuit's declared witnesses.
/// The generic prover binds each slot by its source declaration.
#[derive(Debug, Clone)]
pub enum WitnessSource {
    /// The selected capability's decrypted note field (`note_schema`)
    NoteField(String),
    /// The action's `[[parameters]]` field
    ParamField(String),
    /// The capability's spending key
    Secret,
    /// Merkle inclusion proof (leaf + siblings)
    MerklePath,
    /// Leaf position in the Merkle tree
    LeafPosition,
    /// Fresh blind derived from Seed
    Blind,
    /// Transaction commitment (tx_binding outer)
    TxCommitment,
    /// Transaction nonce
    TxNonce,
}

/// Parsed witness map from the manifest's `[[circuits]]` entry.
#[derive(Debug, Clone)]
pub struct CircuitWitnessMap {
    /// The circuit this map binds to
    pub circuit_name: String,
    /// The contract namespace
    pub namespace: String,
    /// One entry per witness slot, in declared order
    pub entries: Vec<WitnessSource>,
}

impl CircuitWitnessMap {
    /// Parse a `witness_map` from a manifest's circuit declarations.
    /// Each element is a string of the form `"source:field"` or a bare
    /// keyword (`"secret"`, `"merkle_path"`, `"leaf_position"`, `"blind"`,
    /// `"tx_commitment"`, `"tx_nonce"`).
    ///
    /// TODO: full parser — maps each string to the correct `WitnessSource`
    /// variant. The binding rule table above is the specification.
    pub fn from_manifest(
        circuit_name: String,
        namespace: String,
        entries: &[String],
    ) -> Result<Self, crate::error::ProverError> {
        let mut sources = Vec::with_capacity(entries.len());
        for entry in entries {
            sources.push(match entry.as_str() {
                "secret" => WitnessSource::Secret,
                "merkle_path" => WitnessSource::MerklePath,
                "leaf_position" => WitnessSource::LeafPosition,
                "blind" => WitnessSource::Blind,
                "tx_commitment" => WitnessSource::TxCommitment,
                "tx_nonce" => WitnessSource::TxNonce,
                s if s.starts_with("note:") =>
                    WitnessSource::NoteField(s[5..].to_string()),
                s if s.starts_with("param:") =>
                    WitnessSource::ParamField(s[6..].to_string()),
                _ => {
                    return Err(crate::error::ProverError::UnknownWitnessSource(entry.clone()))
                }
            });
        }
        Ok(Self { circuit_name, namespace, entries: sources })
    }

    /// Validate that the witness map count matches the circuit's declared witness count.
    ///
    /// `declared_witness_count` is the number of witness slots decoded from the
    /// zkas binary (`ZkBinary.witnesses.len()`). A mismatch indicates the manifest
    /// and circuit are out of sync and proof generation will fail.
    pub fn validate_count(&self, declared_witness_count: usize) -> Result<(), crate::error::ProverError> {
        if self.entries.len() != declared_witness_count {
            return Err(crate::error::ProverError::WitnessCountMismatch {
                witness_count: self.entries.len(),
                declared_count: declared_witness_count,
            });
        }
        Ok(())
    }
}

/// Trait abstracting over the selected capability's note-schema fields.
/// The `ManifestContractClient` implements this via the wallet's `CapRecord`.
pub trait CapabilityProvider {
    /// Look up a named field from the note schema (e.g., "value" → u64,
    /// "commitment" → pallas::Base). Returns None if the field is not in
    /// this capability's schema.
    fn note_field(&self, name: &str) -> Option<pallas::Base>;
    /// The capability's spending secret, resolved via AccountManager.
    fn secret(&self) -> SecretKey;
    /// The capability's Merkle inclusion proof.
    fn merkle_path(&self) -> Vec<pallas::Base>;
    /// Leaf position for this capability.
    fn leaf_position(&self) -> u32;
}

/// Context the generic prover needs to bind witnesses and create a proof.
/// The concrete implementation in the wallet (`bin/dww`) loads the zkas
/// binary, builds the proving key, binds witnesses per `witness_map`, and
/// creates the proof via `dwow_core::zk`.
pub struct ProverContext {
    /// The contract's manifest — type declarations
    pub manifest: ContractManifest,
    /// The action being invoked
    pub action_name: String,
    /// Parsed witness map for this circuit
    pub witness_map: CircuitWitnessMap,
    /// Transaction Seed (§6.1)
    pub seed: [u8; 32],
}

impl ProverContext {
    /// Construct a ProverContext from the manifest. The zkas binary and
    /// proving key are loaded by the wallet layer (which has `dwow_core`).
    pub fn new(
        manifest: ContractManifest,
        action_name: String,
        witness_map: CircuitWitnessMap,
        seed: [u8; 32],
    ) -> Self {
        Self { manifest, action_name, witness_map, seed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn witness_map_parses_known_sources() {
        let map = CircuitWitnessMap::from_manifest(
            "test".to_string(), "test_ns".to_string(),
            &[
                "secret".to_string(),
                "merkle_path".to_string(),
                "note:value".to_string(),
                "param:amount".to_string(),
                "blind".to_string(),
                "tx_commitment".to_string(),
            ],
        ).unwrap();
        assert_eq!(map.circuit_name, "test");
        assert_eq!(map.namespace, "test_ns");
        assert_eq!(map.entries.len(), 6);
        match &map.entries[2] {
            WitnessSource::NoteField(f) => assert_eq!(f, "value"),
            _ => panic!("expected NoteField"),
        }
        match &map.entries[3] {
            WitnessSource::ParamField(f) => assert_eq!(f, "amount"),
            _ => panic!("expected ParamField"),
        }
    }

    #[test]
    fn witness_map_rejects_unknown() {
        let result = CircuitWitnessMap::from_manifest(
            "test".to_string(), "test_ns".to_string(),
            &["garbage_source".to_string()],
        );
        assert!(result.is_err());
    }
}
