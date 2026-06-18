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
    ) -> Result<(Vec<u8>, Vec<Vec<u8>>), String>;
}

/// Interface the wallet provides to contract clients for reading
/// wallet state (coins, merkle paths, secrets, addresses).
/// Implemented by the wallet's database layer.
pub trait WalletStateProvider: Send + Sync {
    /// Get the default wallet address.
    fn default_address(&self) -> Result<String, String>;

    /// Get unspent coin records for a given token ID.
    fn unspent_coins_for_token(&self, _token_id: &str) -> Result<Vec<CoinInfo>, String> {
        Ok(vec![])
    }
}

/// Minimal coin info passed to contract clients.
pub struct CoinInfo {
    pub coin_id: String,
    pub value: u64,
    pub token_id: String,
    pub leaf_position: u64,
    pub secret: String,       // bs58-encoded
    pub coin_blind: String,   // bs58-encoded
    pub value_blind: String,  // bs58-encoded
    pub token_blind: String,  // bs58-encoded
    pub spend_hook: Option<String>,
    pub user_data: Option<String>,
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
