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
use crate::manifest::{ContractManifest, NoteFieldValue};
use crate::pasta::pallas;

/// The closed derived-witness rule table, mapped 1:1 to the zkas opcode
/// families (wallet.md §6.4.1). A derived slot is computed by the circuit from
/// the already-bound input slots; operands are 0-based witness-slot indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerivedRule {
    /// `poseidon(1, secret, id, nonce)`
    Nullifier { secret: usize, id: usize, nonce: usize },
    /// `poseidon(3, tx_commitment, tx_nonce)`
    TxBinding { txc: usize, txn: usize },
    /// `poseidon(5, id, contents, nonce)`
    Leaf { id: usize, contents: usize, nonce: usize },
    /// `merkle_root(pos, path, poseidon(5, id, contents, nonce))`
    MerkleRoot { pos: usize, path: usize, id: usize, contents: usize, nonce: usize },
    /// `poseidon(7, secret)`
    OwnerPub { secret: usize },
    /// `poseidon(2, asset_id, blind)`
    TokenCommit { asset_id: usize, blind: usize },
    /// `poseidon(4, owner_pub, asset_id, purse_id)`
    PurseId { owner_pub: usize, asset_id: usize, purse_id: usize },
    /// `poseidon(4, coin_public, value, asset_id, spend_hook, user_data, blind)`
    Coin { coin_public: usize, value: usize, asset_id: usize, spend_hook: usize, user_data: usize, blind: usize },
    /// `ec_get_x(pedersen(value, blind))`
    PedersenX { value: usize, blind: usize },
    /// `ec_get_y(pedersen(value, blind))`
    PedersenY { value: usize, blind: usize },
    /// `w[a] + w[b]` (base field)
    BaseAdd { a: usize, b: usize },
    /// `w[a] - w[b]` (base field)
    BaseSub { a: usize, b: usize },
    /// `w[a] + w[b]` (scalar field)
    BlindSum { a: usize, b: usize },
    /// `w[a] - w[b]` (scalar field)
    BlindSub { a: usize, b: usize },
    /// `poseidon(7, secret, nullifier)`
    SignatureSecret { secret: usize, nullifier: usize },
}

/// The manifest annotations that guide witness binding.
///
/// Each entry in the `[[circuits]]` table carries a `witness_map` — one entry
/// per witness slot, in the EXACT order of the circuit's declared witnesses.
/// The generic prover binds each slot by its source declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum WitnessSource {
    /// The selected capability's decrypted note field (`note_schema`)
    NoteField(String),
    /// The action's `[[parameters]]` field
    ParamField(String),
    /// The capability's spending key
    Secret,
    /// A named spending key (multi-secret circuits)
    SecretNamed(String),
    /// Merkle inclusion proof (pre-block root; plain `merkle_path`)
    MerklePath,
    /// Trajectory-relative inclusion proof — pre-block root (§C.6.1)
    MerklePathCurrent,
    /// Trajectory-relative inclusion proof — root after prior ops (§C.6.1)
    MerklePathCumulative,
    /// The Merkle root that anchored the consumed object (wallet's stored proof)
    MerkleRoot,
    /// Leaf position in the Merkle tree
    LeafPosition,
    /// Fresh blind derived from Seed, with a distinct per-name domain
    Blind(String),
    /// Transaction commitment (tx_binding outer)
    TxCommitment,
    /// Transaction nonce
    TxNonce,
    /// A witness the circuit computes from other slots (`derived:<rule>:<slot>,…`)
    Derived(DerivedRule),
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
    /// Each element names the source of one witness slot, in slot order.
    /// The closed vocabulary (wallet.md §6.4.1): input sources
    /// (`note:<field>`, `param:<field>`, `secret[:<name>]`,
    /// `merkle_path[:current|cumulative]`, `leaf_position`, `tx_commitment`,
    /// `tx_nonce`), named blinds (`blind:<name>`), and derived rules
    /// (`derived:<rule>:<slot>[,<slot>…]`). An unknown source is a parse error.
    pub fn from_manifest(
        circuit_name: String,
        namespace: String,
        entries: &[String],
    ) -> Result<Self, crate::error::ProverError> {
        let mut sources = Vec::with_capacity(entries.len());
        for entry in entries {
            sources.push(parse_source(entry)?);
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

/// Parse a single `witness_map` entry string into a [`WitnessSource`].
fn parse_source(entry: &str) -> Result<WitnessSource, crate::error::ProverError> {
    Ok(match entry {
        "secret" => WitnessSource::Secret,
        "merkle_path" => WitnessSource::MerklePath,
        "merkle_path:current" => WitnessSource::MerklePathCurrent,
        "merkle_path:cumulative" => WitnessSource::MerklePathCumulative,
        "merkle_root" => WitnessSource::MerkleRoot,
        "leaf_position" => WitnessSource::LeafPosition,
        "tx_commitment" => WitnessSource::TxCommitment,
        "tx_nonce" => WitnessSource::TxNonce,
        s => {
            if let Some(field) = s.strip_prefix("note:") {
                WitnessSource::NoteField(field.to_string())
            } else if let Some(field) = s.strip_prefix("param:") {
                WitnessSource::ParamField(field.to_string())
            } else if let Some(name) = s.strip_prefix("blind:") {
                WitnessSource::Blind(name.to_string())
            } else if let Some(name) = s.strip_prefix("secret:") {
                WitnessSource::SecretNamed(name.to_string())
            } else if let Some(rule) = s.strip_prefix("derived:") {
                WitnessSource::Derived(parse_derived_rule(rule)?)
            } else {
                return Err(crate::error::ProverError::UnknownWitnessSource(entry.to_string()))
            }
        }
    })
}

/// Parse a `derived:<rule>:<slot>[,<slot>…]` operand list into a [`DerivedRule`].
fn parse_derived_rule(s: &str) -> Result<DerivedRule, crate::error::ProverError> {
    use crate::error::ProverError;
    let (name, ops) = s.split_once(':').ok_or_else(|| {
        ProverError::InvalidDerivedRule(format!("derived:{s} is missing its rule operands"))
    })?;
    let slots = |expected: usize| -> Result<Vec<usize>, ProverError> { parse_slots(ops, expected) };
    Ok(match name {
        "nullifier" => { let a = slots(3)?; DerivedRule::Nullifier { secret: a[0], id: a[1], nonce: a[2] } }
        "tx_binding" => { let a = slots(2)?; DerivedRule::TxBinding { txc: a[0], txn: a[1] } }
        "leaf" => { let a = slots(3)?; DerivedRule::Leaf { id: a[0], contents: a[1], nonce: a[2] } }
        "merkle_root" => { let a = slots(5)?; DerivedRule::MerkleRoot { pos: a[0], path: a[1], id: a[2], contents: a[3], nonce: a[4] } }
        "owner_pub" => { let a = slots(1)?; DerivedRule::OwnerPub { secret: a[0] } }
        "token_commit" => { let a = slots(2)?; DerivedRule::TokenCommit { asset_id: a[0], blind: a[1] } }
        "purse_id" => { let a = slots(3)?; DerivedRule::PurseId { owner_pub: a[0], asset_id: a[1], purse_id: a[2] } }
        "coin" => { let a = slots(6)?; DerivedRule::Coin { coin_public: a[0], value: a[1], asset_id: a[2], spend_hook: a[3], user_data: a[4], blind: a[5] } }
        "pedersen_x" => { let a = slots(2)?; DerivedRule::PedersenX { value: a[0], blind: a[1] } }
        "pedersen_y" => { let a = slots(2)?; DerivedRule::PedersenY { value: a[0], blind: a[1] } }
        "base_add" => { let a = slots(2)?; DerivedRule::BaseAdd { a: a[0], b: a[1] } }
        "base_sub" => { let a = slots(2)?; DerivedRule::BaseSub { a: a[0], b: a[1] } }
        "blind_sum" => { let a = slots(2)?; DerivedRule::BlindSum { a: a[0], b: a[1] } }
        "blind_sub" => { let a = slots(2)?; DerivedRule::BlindSub { a: a[0], b: a[1] } }
        "signature_secret" => { let a = slots(2)?; DerivedRule::SignatureSecret { secret: a[0], nullifier: a[1] } }
        _ => return Err(ProverError::UnknownWitnessSource(format!("derived:{name}"))),
    })
}

/// Parse a comma-separated list of exactly `expected` 0-based slot indices.
fn parse_slots(s: &str, expected: usize) -> Result<Vec<usize>, crate::error::ProverError> {
    use crate::error::ProverError;
    let slots: Result<Vec<usize>, _> =
        s.split(',').map(|t| t.trim().parse::<usize>()).collect();
    let slots = slots.map_err(|_| {
        ProverError::InvalidDerivedRule(format!("derived operand '{s}' has a non-integer slot index"))
    })?;
    if slots.len() != expected {
        return Err(ProverError::InvalidDerivedRule(format!(
            "derived rule expects {expected} operands, got {}",
            slots.len()
        )))
    }
    Ok(slots)
}

/// Trait abstracting over the selected capability's note-schema fields.
/// The `ManifestContractClient` implements this via the wallet's `CapRecord`.
pub trait CapabilityProvider {
    /// Look up a named field from the note schema as its typed value
    /// (`U64`, `Base`, `Scalar`, …). Returns None if the field is absent.
    fn note_value(&self, name: &str) -> Option<NoteFieldValue>;
    /// The capability's spending secret, resolved via AccountManager.
    fn secret(&self) -> SecretKey;
    /// A named spending secret (multi-secret circuits); None if not held.
    fn named_secret(&self, name: &str) -> Option<SecretKey>;
    /// The capability's Merkle inclusion proof (32 siblings).
    fn merkle_path(&self) -> Vec<pallas::Base>;
    /// The Merkle root that anchored this capability (wallet's stored proof).
    fn merkle_root(&self) -> pallas::Base;
    /// Leaf position for this capability.
    fn leaf_position(&self) -> u32;
    /// A named `[[parameters]]` field, typed.
    fn param_value(&self, name: &str) -> Option<NoteFieldValue>;
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
                "blind:quantity".to_string(),
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
        match &map.entries[4] {
            WitnessSource::Blind(name) => assert_eq!(name, "quantity"),
            _ => panic!("expected Blind"),
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

    #[test]
    fn witness_map_parses_named_blind_secret_and_trajectory() {
        let map = CircuitWitnessMap::from_manifest(
            "t".to_string(), "ns".to_string(),
            &[
                "secret:owner".to_string(),
                "merkle_path:current".to_string(),
                "merkle_path:cumulative".to_string(),
                "blind:deposit".to_string(),
            ],
        ).unwrap();
        match &map.entries[0] {
            WitnessSource::SecretNamed(n) => assert_eq!(n, "owner"),
            _ => panic!("expected SecretNamed"),
        }
        assert!(matches!(&map.entries[1], WitnessSource::MerklePathCurrent));
        assert!(matches!(&map.entries[2], WitnessSource::MerklePathCumulative));
        match &map.entries[3] {
            WitnessSource::Blind(n) => assert_eq!(n, "deposit"),
            _ => panic!("expected Blind"),
        }
    }

    #[test]
    fn witness_map_parses_derived_rules() {
        let map = CircuitWitnessMap::from_manifest(
            "t".to_string(), "ns".to_string(),
            &[
                "derived:nullifier:15,0,7".to_string(),
                "derived:tx_binding:19,20".to_string(),
                "derived:pedersen_x:1,2".to_string(),
                "derived:base_add:1,3".to_string(),
                "derived:blind_sum:2,4".to_string(),
            ],
        ).unwrap();
        assert_eq!(
            map.entries[0],
            WitnessSource::Derived(DerivedRule::Nullifier { secret: 15, id: 0, nonce: 7 })
        );
        assert_eq!(
            map.entries[1],
            WitnessSource::Derived(DerivedRule::TxBinding { txc: 19, txn: 20 })
        );
        assert_eq!(
            map.entries[2],
            WitnessSource::Derived(DerivedRule::PedersenX { value: 1, blind: 2 })
        );
        assert_eq!(
            map.entries[3],
            WitnessSource::Derived(DerivedRule::BaseAdd { a: 1, b: 3 })
        );
        assert_eq!(
            map.entries[4],
            WitnessSource::Derived(DerivedRule::BlindSum { a: 2, b: 4 })
        );
    }

    #[test]
    fn witness_map_rejects_bad_derived_arity() {
        let result = CircuitWitnessMap::from_manifest(
            "t".to_string(), "ns".to_string(),
            &["derived:tx_binding:19".to_string()],  // expects 2 operands
        );
        assert!(result.is_err());
        let result = CircuitWitnessMap::from_manifest(
            "t".to_string(), "ns".to_string(),
            &["derived:nullifier:a,b,c".to_string()],  // non-integer
        );
        assert!(result.is_err());
    }
}
