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
//! Note: DarkWow has no "coins." There are native tokens (consensus asset)
//! and promissory notes (capabilities). Both are capabilities in the o-cap
//! model. Types use "note" terminology, not "coin."

use std::collections::HashMap;

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
    /// - `wallet_state`: provider for wallet state (coins, merkle paths, secrets)
    ///
    /// Returns Ok((call_data, proof_bytes)) where call_data is the serialized
    /// function parameters (NOT including the function code byte).
    /// proof_bytes are raw ZK proof byte vectors.
    /// The wallet prepends the function code byte.
    fn build(
        &self,
        function: &str,
        params: &str,
        wallet_state: &dyn WalletStateProvider,
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
/// wallet state (notes, merkle paths, secrets, addresses).
/// Implemented by the wallet's database layer.
pub trait WalletStateProvider: Send + Sync {
    /// Get the default wallet address.
    fn default_address(&self) -> std::result::Result<String, String>;

    /// Get unspent note records for a given token ID.
    fn unspent_notes_for_token(&self, _token_id: &str) -> std::result::Result<Vec<NoteInfo>, String> {
        Ok(vec![])
    }

    /// Get the Merkle proof for a note by its note_id.
    /// Returns the proof siblings (bs58-encoded) and leaf position.
    fn get_merkle_proof(&self, _note_id: &str) -> std::result::Result<MerkleProofInfo, String> {
        Err("get_merkle_proof not implemented".to_string())
    }

    /// Get the default wallet secret key (bs58-encoded).
    fn get_secret(&self) -> std::result::Result<String, String> {
        Err("get_secret not implemented".to_string())
    }
}

/// Merkle proof info passed to contract clients for ZK witness construction.
pub struct MerkleProofInfo {
    /// bs58-encoded sibling nodes (32 per Merkle tree depth)
    pub siblings: Vec<String>,
    /// Leaf position in the Merkle tree
    pub leaf_position: u64,
}

/// Minimal note info passed to contract clients.
pub struct NoteInfo {
    pub note_id: String,
    pub value: u64,
    pub token_id: String,
    pub leaf_position: u64,
    pub secret: String,       // bs58-encoded
    pub note_blind: String,   // bs58-encoded
    pub value_blind: String,  // bs58-encoded
    pub token_blind: String,  // bs58-encoded
    pub spend_hook: Option<String>,
    pub user_data: Option<String>,
}

/// A held capability — passed to ContractClient::detect_transferred()
/// so each contract can match its call data signatures against held secrets.
pub struct CapabilityInfo {
    /// Unique capability identifier (e.g., note_id for PN notes)
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

    pub fn find_by_name(&self, name: &str) -> Option<&(dyn ContractClient + Send + Sync)> {
        self.get(name)
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
