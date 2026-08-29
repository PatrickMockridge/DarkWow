/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Generic contract client trait — the interface between the wallet's
//! capability kernel and each contract's ZK proof builders.
//!
//! Each contract implements this trait in its own crate
//! (`src/contract/<name>/src/client/`). The wallet dispatches generically
//! through this trait — no per-contract logic in the wallet.
//!
//! Architecture:
//!   Wallet → ContractClient::build(function, params, wallet_state) → (call_data, proofs)
//!   The contract's client loads its ZK binary, builds witnesses from
//!   wallet state, generates the proof, and encodes parameters.
//!   The wallet only attaches the fee and broadcasts the transaction.
//!
//! Note: DarkWow has no "coins." At the wallet level there are exactly two
//! categories: native tokens (consensus asset) and capabilities (everything else).
//! Types use generic o-cap terminology — "cap" for capability, not "coin" or "note."

use std::collections::HashMap;
use crate::crypto::{BaseBlind, FuncId, ScalarBlind, AssetId};

/// Interface for contract function builders.
/// Each contract implements this in its own crate.
pub trait ContractClient: Send + Sync {
    /// Human-readable contract name (e.g. "native_token", "promissory_note").
    fn contract_name(&self) -> &'static str;

    /// Return the function code byte for a named function.
    /// Returns None if the function is not supported by this contract.
    fn function_selector(&self, function: &str) -> Option<u8>;

    /// List all supported function names.
    fn supported_functions(&self) -> Vec<&'static str>;

    /// Build call data and ZK proofs for a contract function.
    ///
    /// - `function`: function name (e.g., "create_escrow", "TransferV1")
    /// - `params`: JSON-encoded parameters for the function
    /// - `wallet_state`: provider for wallet state (held capabilities, Merkle paths, secrets)
    ///
    /// Returns Ok((call_data, proof_bytes)) where call_data is the serialized
    /// function parameters (NOT including the function code byte).
    /// proof_bytes are raw ZK proof byte vectors.
    /// The wallet prepends the function code byte.
    fn build(
        &self,
        function: &str,
        params: &str,
        _wallet_state: &dyn WalletStateProvider,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String>;

    /// Detect which held capabilities were transferred (exercised/consumed)
    /// by this contract's call data. Each contract's client knows how to
    /// decode its own call data and match signatures against held secrets.
    ///
    /// Returns the capability IDs that were consumed.
    /// Default: no capabilities detected (non-consumable contracts, e.g. deployooor).
    fn detect_transferred(
        &self,
        _call_data: &[u8],
        _held_capabilities: &[CapabilityInfo],
    ) -> Vec<String> {
        vec![]
    }
}

impl core::fmt::Debug for dyn ContractClient + Send + Sync {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ContractClient")
            .field("name", &self.contract_name())
            .finish()
    }
}

/// Interface the wallet provides to contract clients for reading
/// wallet state (held capabilities, Merkle paths, secrets, addresses).
/// Implemented by the wallet's database layer.
pub trait WalletStateProvider: Send + Sync {
    /// Get the default wallet address.
    fn default_address(&self) -> std::result::Result<String, String>;

    /// Get held capability records for a given asset ID.
    fn held_capabilities_by_asset(&self, _asset_id: &str) -> std::result::Result<Vec<CapInfo>, String> {
        Ok(vec![])
    }

    /// Get the Merkle proof for a capability by its cap_id.
    /// Returns the proof siblings (bs58-encoded) and leaf position.
    fn get_merkle_proof(&self, _cap_id: &str) -> std::result::Result<MerkleProofInfo, String> {
        Err("get_merkle_proof not implemented".to_string())
    }

    /// Get the default wallet secret key (bs58-encoded).
    fn get_secret(&self) -> std::result::Result<String, String> {
        Err("get_secret not implemented".to_string())
    }

    /// Load a zkas circuit binary from the wallet's zkas_binaries store
    /// (wallet.md §3, §6.4.1 step 3). Keyed by (contract_id bs58, namespace,
    /// circuit_name). Returns None if not found.
    fn load_zkas_binary(
        &self,
        _contract_id: &str,
        _namespace: &str,
        _circuit_name: &str,
    ) -> Option<Vec<u8>> {
        None
    }

    /// Generate a ZK proof from a manifest-declared circuit.
    ///
    /// The wallet implements this via its concrete ProverImpl (which has
    /// `dwow_core::zk` access). The SDK defines the prover context and
    /// witness-binding logic; the wallet provides the ZK machinery.
    ///
    /// `params` are the action's `[[parameters]]` values (typed, decoded from
    /// the JSON params by the caller) — bound as `param:<field>` witnesses.
    ///
    /// Returns the encoded proof bytes, or an error string.
    fn generate_proof(
        &self,
        _contract_id: &str,
        _witness_map: &crate::prover::CircuitWitnessMap,
        _zkas_bytes: &[u8],
        _seed: [u8; 32],
        _params: &[(String, crate::manifest::NoteFieldValue)],
    ) -> Result<(Vec<u8>, Vec<Option<crate::manifest::NoteFieldValue>>), String> {
        Err("generate_proof not implemented — wallet must provide concrete ProverImpl".to_string())
    }
}

/// Merkle proof info passed to contract clients for ZK witness construction.
pub struct MerkleProofInfo {
    /// bs58-encoded sibling nodes (32 per Merkle tree depth)
    pub siblings: Vec<String>,
    /// bs58-encoded Merkle root anchoring the capability
    pub root: String,
    /// Leaf position in the Merkle tree
    pub leaf_position: u64,
}

/// Minimal capability info passed to contract clients.
/// Represents a held capability discovered via AEAD decryption —
/// generic across all contracts, not specific to any one.
pub struct CapInfo {
    pub cap_id: String,
    pub value: u64,
    /// AssetId (↓denominate) — typed per type-system.md §8.1
    pub asset_id: AssetId,
    pub leaf_position: u64,
    pub secret: String,       // bs58-encoded (per Cornerstone 1, secrets in memory)
    /// BaseBlind — capability commitment blinding factor
    pub cap_blind: BaseBlind,
    /// ScalarBlind — value blinding factor
    pub value_blind: ScalarBlind,
    /// BaseBlind — asset blinding factor
    pub asset_blind: BaseBlind,
    /// FuncId (↓gate) — None for pre-V.1 records
    pub spend_hook: Option<FuncId>,
    /// Raw user data field element
    pub user_data: Option<[u8; 32]>,
}

/// A held capability — passed to ContractClient::detect_transferred()
/// so each contract can match its call data signatures against held secrets.
pub struct CapabilityInfo {
    /// Unique capability identifier (e.g., cap_id for a held PN note)
    pub capability_id: String,
    /// Holder's secret key (bs58-encoded)
    pub secret: String,
}

/// Registry of contract clients, keyed by contract name.
/// Each contract crate registers its client at startup.
pub struct ContractClientRegistry {
    clients: HashMap<String, Box<dyn ContractClient + Send + Sync>>,
}

impl ContractClientRegistry {
    pub fn new() -> Self {
        Self { clients: HashMap::new() }
    }

    pub fn register(&mut self, name: &str, client: Box<dyn ContractClient + Send + Sync>) {
        self.clients.insert(name.to_string(), client);
    }

    pub fn get(&self, name: &str) -> Option<&(dyn ContractClient + Send + Sync)> {
        self.clients.get(name).map(|c| c.as_ref())
    }
}

impl Default for ContractClientRegistry {
    fn default() -> Self { Self::new() }
}

/// A generic ContractClient for contracts that don't yet have a specialized
/// client with ZK proof generation. Uses a static function→opcode table.
///
/// Matches `GenericContractClient` in the Python model (wallet_model.py:6895).
/// Each contract crate can later replace this with a specialized client that
/// loads ZK binaries and generates real proofs.
pub struct GenericContractClient {
    name: &'static str,
    /// (function_name, opcode) pairs
    functions: &'static [(&'static str, u8)],
}

impl GenericContractClient {
    pub const fn new(name: &'static str, functions: &'static [(&'static str, u8)]) -> Self {
        Self { name, functions }
    }
}

impl ContractClient for GenericContractClient {
    fn contract_name(&self) -> &'static str {
        self.name
    }

    fn function_selector(&self, function: &str) -> Option<u8> {
        self.functions.iter()
            .find(|(name, _)| *name == function)
            .map(|(_, code)| *code)
    }

    fn supported_functions(&self) -> Vec<&'static str> {
        self.functions.iter().map(|(name, _)| *name).collect()
    }

    fn build(&self, function: &str, _params: &str, _wallet_state: &dyn WalletStateProvider) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        if self.function_selector(function).is_some() {
            Ok((vec![], vec![]))
        } else {
            Err(format!("{}: unsupported function '{}'", self.name, function))
        }
    }
}

/// A ContractClient derived from a contract's on-chain manifest.
/// Generic — works for any contract with a stored manifest. No per-contract code.
pub struct ManifestContractClient {
    manifest: crate::manifest::ContractManifest,
    name: &'static str,
    /// ContractId (bs58) — threaded to the wallet's zkas/proof store so the
    /// generic prover can resolve the manifest and zkas binary (wallet.md §6.4.1).
    contract_id: String,
    /// The shell's transaction Seed (§6.1) — every blind and proving randomness
    /// derives from it. Never hardcoded (write-path invariant 1).
    seed: [u8; 32],
    /// The produce-side note's recipient (the new owner's key). The note is
    /// encrypted to this key; it is not a wire param (never serialized).
    recipient: crate::crypto::PublicKey,
}

impl ManifestContractClient {
    pub fn new(
        name: &'static str,
        manifest: crate::manifest::ContractManifest,
        contract_id: String,
        seed: [u8; 32],
        recipient: crate::crypto::PublicKey,
    ) -> Self {
        Self { manifest, name, contract_id, seed, recipient }
    }

    /// Emit the produce-side AEAD note (the Create phase): resolve the action's
    /// `produces` capability `note_schema`, fill each field from the decoded
    /// params + the prover's bound values (per the `source`/`witness` tags), and
    /// encrypt to `self.recipient` with a Seed-derived ephemeral secret (§6.1).
    /// Returns the serialized encrypted note (empty when the action produces no
    /// note).
    fn emit_produce_note(
        &self,
        func_code: u8,
        decoded_params: &[(String, crate::manifest::NoteFieldValue)],
        bound_values: &[Option<crate::manifest::NoteFieldValue>],
    ) -> Result<Vec<u8>, String> {
        let note_schema = self.manifest.note_schema_for_function(func_code).unwrap_or(&[]);
        if note_schema.is_empty() {
            return Ok(Vec::new())
        }
        let mut note_fields = Vec::with_capacity(note_schema.len());
        for field in note_schema {
            let value = if let Some(d) = &field.derived {
                // derived:<rule>:<slot> — currently `increment:<slot>` = bound[slot] + 1
                // (the purse's in-circuit nonce increment, HAZOP V1/V2).
                let (rule, slot_str) = d.split_once(':').ok_or_else(||
                    format!("note field '{}': malformed derived '{d}'", field.name))?;
                if rule != "increment" {
                    return Err(format!("note field '{}': unsupported derived rule '{rule}'", field.name));
                }
                let slot: usize = slot_str.trim().parse().map_err(|e|
                    format!("note field '{}': bad derived slot '{slot_str}': {e}", field.name))?;
                let b = bound_values.get(slot).and_then(|v| v.clone())
                    .ok_or_else(|| format!("note field '{}': derived slot {slot} unbound", field.name))?;
                match b {
                    crate::manifest::NoteFieldValue::Base(b) =>
                        crate::manifest::NoteFieldValue::Base(b + crate::pasta::pallas::Base::one()),
                    other => return Err(format!(
                        "note field '{}': derived increment on non-Base value {other:?}", field.name)),
                }
            } else if let Some(slot) = field.witness {
                bound_values.get(slot).and_then(|v| v.clone())
                    .ok_or_else(|| format!("note field '{}': witness slot {slot} unbound", field.name))?
            } else {
                let src = field.source.as_deref().unwrap_or(&field.name);
                decoded_params.iter().find(|(n, _)| n == src).map(|(_, v)| v.clone())
                    .ok_or_else(|| format!("note field '{}': source param '{src}' not found", field.name))?
            };
            note_fields.push((field.name.clone(), value));
        }
        let plaintext = crate::manifest::encode_params_values(note_schema, &note_fields)
            .map_err(|e| format!("note encode: {e}"))?;
        let ephem = crate::crypto::SecretKey::from_base(
            crate::crypto::util::hash_to_base(b"darkwow-note-eph", &[&self.seed]),
        );
        let note = crate::crypto::note::AeadEncryptedNote::encrypt_deterministic(
            &RawBytes(plaintext), &self.recipient, ephem,
        ).map_err(|e| format!("note encrypt: {e:?}"))?;
        let mut note_bytes = Vec::new();
        dwow_serial::Encodable::encode(&note, &mut note_bytes)
            .map_err(|e| format!("note serialize: {e}"))?;
        Ok(note_bytes)
    }
}

/// An `Encodable` wrapper that writes raw plaintext bytes with no length prefix —
/// the AEAD note plaintext is the positional `note_schema` field bytes, not a
/// length-prefixed `Vec<u8>`.
struct RawBytes(Vec<u8>);
impl dwow_serial::Encodable for RawBytes {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> {
        w.write_all(&self.0)?;
        Ok(self.0.len())
    }
}

impl ContractClient for ManifestContractClient {
    fn contract_name(&self) -> &'static str {
        self.name
    }

    fn function_selector(&self, function: &str) -> Option<u8> {
        self.manifest.functions.iter()
            .find(|f| f.name == function)
            .map(|f| f.code)
    }

    fn supported_functions(&self) -> Vec<&'static str> {
        self.manifest.functions.iter()
            .map(|f| Box::leak(f.name.clone().into_boxed_str()) as &'static str)
            .collect()
    }

    fn build(
        &self,
        function: &str,
        params: &str,
        wallet_state: &dyn WalletStateProvider,
    ) -> Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let func = self.manifest.functions.iter()
            .find(|f| f.name == function)
            .ok_or_else(|| format!(
                "{}: unknown function '{}'", self.name, function
            ))?;

        // Resolve the manifest's parameter schema for this function (wire order).
        let param_schema: Vec<crate::manifest::ParameterField> = self
            .manifest
            .parameters
            .iter()
            .find(|p| p.function == function)
            .map(|p| p.fields.clone())
            .unwrap_or_default();

        // Decode the JSON params into typed values for the prover's
        // `param:<field>` binding and the wire-param assembly.
        #[cfg(feature = "json")]
        let mut decoded_params = crate::manifest::decode_params_from_json(
            &param_schema, params,
        ).map_err(|e| format!("{}: '{}' parameter decoding: {}", self.name, function, e))?;
        #[cfg(not(feature = "json"))]
        let mut decoded_params: Vec<(String, crate::manifest::NoteFieldValue)> = Vec::new();

        // circuit_registry route removed (D2 — phantom-code-removed-first).
        // The generic prover (wallet.md §6.4.1) builds proofs from the
        // zkas binary + manifest witness_map — no compiled-in builder.

        let mut bound_values: Vec<Option<crate::manifest::NoteFieldValue>> = Vec::new();
        let proof_bytes: Vec<u8> = if func.requires_proof {
            let circuit_name = func.proof_circuit.as_deref().unwrap_or("none");
            let circuit = self.manifest.circuits.iter()
                .find(|c| c.name == circuit_name)
                .ok_or_else(|| format!(
                    "{}: '{}' requires proof circuit '{}' but manifest has no [[circuits]] entry for it",
                    self.name, function, circuit_name,
                ))?;

            let witness_map = crate::prover::CircuitWitnessMap::from_manifest(
                circuit.name.clone(),
                circuit.namespace.clone(),
                &circuit.witness_map,
            ).map_err(|e| format!(
                "{}: '{}' witness_map error for circuit '{}': {}",
                self.name, function, circuit_name, e,
            ))?;

            let zkas_bytes = wallet_state.load_zkas_binary(
                &self.contract_id,
                &circuit.namespace,
                circuit_name,
            ).ok_or_else(|| format!(
                "{}: '{}' requires ZK proof (circuit: {}) but zkas binary not found \
                 in store — contract may not be deployed or the circuit is not \
                 embedded/extracted",
                self.name, function, circuit_name,
            ))?;

            // Delegate to the wallet's concrete ProverImpl (§6.4.1 steps 4-6);
            // it returns the proof plus the circuit-computed witness values.
            let (proof, bv) = wallet_state.generate_proof(
                &self.contract_id,
                &witness_map,
                &zkas_bytes,
                self.seed, // §6.1 — the shell's Seed, never hardcoded
                &decoded_params,
            ).map_err(|e| format!(
                "{}: '{}' generic prover failed: {}", self.name, function, e,
            ))?;
            bound_values = bv;

            // Params assembly: fill the circuit-computed (witness-tagged) wire
            // fields from the prover's bound values (typed — Base or Scalar).
            for field in &param_schema {
                if let Some(slot) = field.witness {
                    let nv = bound_values.get(slot).and_then(|v| v.clone()).ok_or_else(|| format!(
                        "{}: '{}' witness slot {} for param '{}' is unbound",
                        self.name, function, slot, field.name,
                    ))?;
                    decoded_params.push((field.name.clone(), nv));
                }
            }

            proof
        } else {
            Vec::new()
        };

        // Encode the full (user + computed) params into positional wire bytes.
        #[cfg(feature = "json")]
        let encoded_params = crate::manifest::encode_params_values(
            &param_schema, &decoded_params,
        ).map_err(|e| format!("{}: '{}' parameter encoding: {}", self.name, function, e))?;
        #[cfg(not(feature = "json"))]
        let encoded_params: Vec<u8> = {
            let _ = (&param_schema, &decoded_params, wallet_state, &func);
            return Err("JSON parameter encoding not available (enable 'json' feature)".into());
        };

        // Emit the produce-side AEAD note (Create phase) and append it to the
        // call data — the scan byte-slides over call.data for the note.
        let note_bytes = if func.requires_proof {
            self.emit_produce_note(func.code, &decoded_params, &bound_values)?
        } else {
            Vec::new()
        };
        let mut call_data = encoded_params;
        call_data.extend_from_slice(&note_bytes);

        if proof_bytes.is_empty() {
            Ok((call_data, vec![]))
        } else {
            Ok((call_data, vec![proof_bytes]))
        }
    }
}
