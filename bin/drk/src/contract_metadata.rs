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

use dwow_sdk::crypto::ContractId;
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
    /// Name of the proof circuit for ZK proof generation (e.g., "init_v1", "transfer_v1")
    pub proof_circuit: Option<&'static str>,
}

/// Metadata for a single contract containing all its functions
#[derive(Debug, Clone)]
pub struct ContractMetadata {
    /// Human-readable contract name (e.g., "dao_escrow", "money_v3")
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
        // Money V3 Contract
        let money_v3 = ContractMetadata {
            name: "money_v3",
            functions: vec![
                FunctionSignature { name: "token_mint", code: 0x00, requires_proof: true, proof_circuit: Some("token_mint_v1") },
                FunctionSignature { name: "auth_token_mint", code: 0x01, requires_proof: true, proof_circuit: Some("auth_token_mint_v1") },
                FunctionSignature { name: "mint", code: 0x02, requires_proof: true, proof_circuit: Some("mint_v1") },
                FunctionSignature { name: "burn", code: 0x03, requires_proof: true, proof_circuit: Some("burn_v1") },
                FunctionSignature { name: "transfer", code: 0x04, requires_proof: false, proof_circuit: None },
            ],
        };
        self.contracts.insert("money_v3", money_v3);

        // DAO-Escrow Contract
        let dao_escrow = ContractMetadata {
            name: "dao_escrow",
            functions: vec![
                FunctionSignature { name: "initialize", code: 0x00, requires_proof: true, proof_circuit: Some("init_v1") },
                FunctionSignature { name: "update", code: 0x01, requires_proof: false, proof_circuit: None },
                FunctionSignature { name: "pay_premium", code: 0x02, requires_proof: true, proof_circuit: Some("pay_premium_v1") },
                FunctionSignature { name: "withdraw", code: 0x03, requires_proof: false, proof_circuit: None },
                FunctionSignature { name: "endowment_withdraw", code: 0x04, requires_proof: false, proof_circuit: None },
                FunctionSignature { name: "treasury_spend", code: 0x05, requires_proof: false, proof_circuit: None },
                FunctionSignature { name: "enable_drain_protection", code: 0x06, requires_proof: false, proof_circuit: None },
                FunctionSignature { name: "propose_claim", code: 0x07, requires_proof: false, proof_circuit: None },
                FunctionSignature { name: "vote_claim", code: 0x08, requires_proof: false, proof_circuit: None },
            ],
        };
        self.contracts.insert("dao_escrow", dao_escrow);

        // DrainProtection Contract
        let drain_protection = ContractMetadata {
            name: "drain_protection",
            functions: vec![
                FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
            ],
        };
        self.contracts.insert("drain_protection", drain_protection);

        // Deployooor Contract
        let deployooor = ContractMetadata {
            name: "deployooor",
            functions: vec![
                FunctionSignature { name: "deploy", code: 0x00, requires_proof: false, proof_circuit: None },
                FunctionSignature { name: "lock", code: 0x01, requires_proof: false, proof_circuit: None },
            ],
        };
        self.contracts.insert("deployooor", deployooor);
    }

    /// Look up contract metadata by contract name
    pub fn get(&self, name: &str) -> Option<&ContractMetadata> {
        self.contracts.get(name)
    }

    /// Look up a specific function within a contract
    pub fn get_function(&self, contract_name: &str, function_name: &str) -> Option<&FunctionSignature> {
        self.get(contract_name).and_then(|c| c.get_function(function_name))
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