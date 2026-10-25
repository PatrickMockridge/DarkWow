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

//! Contract Metadata Registry
//!
//! Static definitions of all known contract functions for universal contract interaction.
//! WASM contracts don't support runtime introspection, so we use static metadata.
//!
//! ## Design Rationale
//!
//! - **Static metadata**: WASM contracts don't expose function enumeration at runtime
//! - **Binary serialization**: Params use `SerialEncodable`, not JSON - we deserialize from JSON
//!   into the native type, then serialize to binary for the contract call
//! - **ZK proof generation**: Contract-specific, handled per-function in `invoke_contract`

use std::collections::HashMap;

/// Represents a single contract function signature
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Human-readable function name (e.g., "initialize", "transfer")
    pub name: &'static str,
    /// Function code byte used in contract call data
    pub code: u8,
    /// Whether this function requires ZK proof generation
    pub requires_proof: bool,
    /// Name of the proof circuit for ZK proof generation (e.g., "Fee_V3", "init_v1")
    pub proof_circuit: Option<&'static str>,
}

/// Metadata for a single contract containing all its functions
#[derive(Debug, Clone)]
pub struct ContractMetadata {
    /// Human-readable contract name (e.g., "dao_escrow", "promissory_note")
    pub name: &'static str,
    /// List of all functions this contract supports
    pub functions: Vec<FunctionSignature>,
}

impl ContractMetadata {
    /// Look up a function by name
    pub fn get_function(&self, name: &str) -> Option<&FunctionSignature> {
        self.functions.iter().find(|f| f.name == name)
    }
}

/// Registry of all known contracts and their functions
pub struct ContractMetadataRegistry {
    /// Map from contract name to contract metadata
    contracts: HashMap<&'static str, ContractMetadata>,
}

impl ContractMetadataRegistry {
    /// Create a new registry with all known contracts pre-registered
    pub fn new() -> Self {
        let mut registry = Self { contracts: HashMap::new() };
        registry.register_known_contracts();
        registry
    }

    /// Register all known DarkWow contracts
	fn register_known_contracts(&mut self) {
		// Native Token Contract (DRKW token - fees and native operations)
		// Hardcoded infrastructure — the native_token manifest.toml is FYI-only
		// (not used for capability discovery or construction).
		let native_token = ContractMetadata {
			name: "native_token",
			functions: vec![
				// 0x01 (mint) is absent on purpose: it is declared in the contract's
				// enum but every dispatch arm returns InvalidFunction, so no accepted
				// block can carry one and the wallet will never see one.
				FunctionSignature { name: "burn", code: 0x02, requires_proof: true, proof_circuit: Some("Burn_V2") },
				FunctionSignature { name: "transfer", code: 0x03, requires_proof: true, proof_circuit: None },
				FunctionSignature { name: "spend", code: 0x04, requires_proof: true, proof_circuit: None },
				FunctionSignature { name: "pow_reward", code: 0x05, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "fee_collect", code: 0x06, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "uncle_mint", code: 0x07, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "fee", code: 0x08, requires_proof: true, proof_circuit: Some("Fee_V3") },
			],
		};
		self.contracts.insert("native_token", native_token);

		// DAO-Escrow is NOT registered: the wallet dropped per-contract handling for
		// it (lib.rs §"dao_escrow and drain_protection removed — the wallet uses
		// generic AEAD scan + manifest"). The entry that used to be here also named
		// V1 circuits (`init_v1`, `pay_premium_v1`) that no longer exist, so it would
		// have shadowed the contract's real manifest with stale data.

		// Deployooor Contract
		let deployooor = ContractMetadata {
			name: "deployooor",
			functions: vec![
				FunctionSignature { name: "deploy", code: 0x00, requires_proof: false, proof_circuit: None },
				// Deployooor is plaintext infrastructure: neither endpoint has a circuit.
				// Authorization is the deploy key's transaction signature, declared by
				// each endpoint's `get_metadata`.
				FunctionSignature { name: "lock", code: 0x01, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("deployooor", deployooor);
    }

    /// Look up a contract by name
    pub fn get(&self, name: &str) -> Option<&ContractMetadata> {
        self.contracts.get(name)
    }

    /// Look up a specific function within a contract
    pub fn get_function(&self, contract_name: &str, function_name: &str) -> Option<&FunctionSignature> {
        self.contracts.get(contract_name).and_then(|c| c.get_function(function_name))
    }

    /// Reverse-lookup: find contract name by its registered ContractId.
    /// Used by contract invoke to resolve Base58 contract IDs back to names.
    pub fn find_by_contract_id(&self, cid: &dwow_sdk::crypto::ContractId) -> Option<&'static str> {
        for (name, _meta) in &self.contracts {
            if crate::contract_imports::get_contract_id(name)
                .map(|registered_id| registered_id == *cid)
                .unwrap_or(false)
            {
                return Some(name);
            }
        }
        None
    }

    /// List all registered contract names
    pub fn contract_names(&self) -> Vec<&'static str> {
        self.contracts.keys().copied().collect()
    }
}

impl Default for ContractMetadataRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global contract metadata registry singleton using lazy initialization
pub static CONTRACT_METADATA_REGISTRY: std::sync::LazyLock<ContractMetadataRegistry> =
    std::sync::LazyLock::new(ContractMetadataRegistry::new);

#[cfg(test)]
mod tests {
    use super::*;

    /// OBL-C214: the wallet's hardcoded registry and the contract `manifest.toml` are two homes
    /// for the same function table. They were never compared, so either could drift silently.
    ///
    /// Two exceptions are declared in `register_known_contracts` and checked here as exceptions:
    ///
    /// * `native_token::mint` is declared in the manifest's enum but every dispatch arm refuses
    ///   it, so no accepted block can carry one and the wallet registry omits it on purpose. If
    ///   the manifest drops it, this test says so.
    /// * `deployooor` is plaintext infrastructure with no `manifest.toml`; if one appears, the
    ///   exception is stale and the comparison must be enabled for it.
    #[test]
    fn registry_matches_manifests() {
        let registry = ContractMetadataRegistry::new();
        let contracts_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/contract");
        let mut compared = 0usize;

        for (name, meta) in &registry.contracts {
            let manifest_path = contracts_dir.join(name).join("manifest.toml");
            if *name == "deployooor" {
                assert!(
                    !manifest_path.is_file(),
                    "deployooor now has a manifest; remove the exception and compare it"
                );
                continue;
            }
            let text = std::fs::read_to_string(&manifest_path).unwrap_or_else(|e| {
                panic!("registry contract {name} has no readable manifest at {manifest_path:?}: {e}")
            });
            let manifest = dwow_sdk::manifest::ContractManifest::from_toml(&text)
                .unwrap_or_else(|e| panic!("{name}/manifest.toml does not parse: {e}"));

            for f in &manifest.functions {
                if *name == "native_token" && f.name == "mint" {
                    // The declared exception: present in the enum, refused by every arm.
                    continue;
                }
                let sig = meta.get_function(&f.name).unwrap_or_else(|| {
                    panic!("{name}: manifest function '{}' is missing from the wallet registry", f.name)
                });
                assert_eq!(sig.code, f.code, "{name}::{} code drift", f.name);
                assert_eq!(
                    sig.requires_proof, f.requires_proof,
                    "{name}::{} requires_proof drift",
                    f.name
                );
                // A multi-proof arm declares its ordered set; the registry's single
                // `proof_circuit` is `None` for those by convention (OBL-C260 tracks the
                // multi-proof call plan), so only single-circuit arms are compared here.
                if f.proof_circuits.is_empty() {
                    assert_eq!(
                        sig.proof_circuit,
                        f.proof_circuit.as_deref(),
                        "{name}::{} proof circuit drift",
                        f.name
                    );
                }
            }
            for sig in &meta.functions {
                assert!(
                    manifest.functions.iter().any(|f| f.name == sig.name),
                    "{name}: registry function '{}' is missing from the manifest",
                    sig.name
                );
            }
            compared += 1;
        }

        // A comparison that compared nothing is not a control.
        assert!(compared >= 1, "the registry comparison did not run");
    }
}
