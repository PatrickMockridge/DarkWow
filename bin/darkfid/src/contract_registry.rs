/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 0-2026 Dyne.org foundation
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

//! Contract registry for generalized contract invocation.
//!
//! This module provides a registry that maps contract identifiers
//! to their handler implementations, enabling generalized contract
//! invocation through a single RPC endpoint.

use std::collections::HashMap;

use darkfi_sdk::crypto::ContractId;
use tinyjson::JsonValue;
use tracing::error;

use crate::{wallet::Wallet, RpcError};

/// Result type for contract handler operations
pub type HandlerResult<T> = Result<T, ContractHandlerError>;

/// Errors that can occur during contract handling
#[derive(Debug, thiserror::Error)]
pub enum ContractHandlerError {
    #[error("Contract not found: {0}")]
    ContractNotFound(String),
    #[error("Function not found: {0}")]
    FunctionNotFound(String),
    #[error("Failed to build params: {0}")]
    ParamsBuildFailed(String),
    #[error("Failed to generate proofs: {0}")]
    ProofGenerationFailed(String),
    #[error("Failed to serialize: {0}")]
    SerializationFailed(String),
    #[error("Wallet error: {0}")]
    WalletError(String),
    #[error("Invalid params: {0}")]
    InvalidParams(String),
}

/// Trait for contract handlers.
///
/// Each contract implementation (money, dao_escrow, etc.)
/// provides a handler that can build calls and generate proofs.
pub trait ContractHandler: Send + Sync {
    /// Returns the contract identifier string (e.g., "dao_escrow", "money")
    fn contract_id(&self) -> &'static str;

    /// Returns the function selector (first byte of calldata) for a given function name.
    /// Returns None if the function is not supported.
    fn function_selector(&self, function: &str) -> Option<u8>;

    /// Build the calldata bytes from JSON params and function selector.
    /// The first byte of the returned vec should be the function selector.
    fn build_params(&self, function: &str, params: JsonValue) -> HandlerResult<Vec<u8>>;

    /// Get the list of functions supported by this handler.
    fn supported_functions(&self) -> Vec<&'static str>;
}

/// Registry of available contract handlers.
pub struct ContractRegistry {
    handlers: HashMap<String, Box<dyn ContractHandler>>,
}

impl ContractRegistry {
    /// Create a new registry with default handlers registered.
    pub fn new() -> Self {
        let mut registry = Self { handlers: HashMap::new() };
        registry.register_default_handlers();
        registry
    }

    /// Register a new handler.
    pub fn register(&mut self, handler: Box<dyn ContractHandler>) {
        self.handlers.insert(handler.contract_id().to_string(), handler);
    }

    /// Get a handler by contract ID string.
    pub fn get(&self, contract_id: &str) -> Option<&dyn ContractHandler> {
        self.handlers.get(contract_id).map(|h| h.as_ref())
    }

    /// Register default handlers.
    fn register_default_handlers(&mut self) {
        // TODO: Add handlers as they are implemented
        // self.handlers.insert("money".to_string(), Box::new MoneyContractHandler::new()));
        // self.handlers.insert("dao_escrow".to_string(), Box::new(DaoEscrowContractHandler::new())));
    }
}

impl Default for ContractRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve a contract identifier string to a ContractId.
///
/// For native contracts (money, dao, deployooor), returns the hardcoded ID.
/// For WASM contracts, looks up the ID from the blockchain.
pub async fn resolve_contract_id(
    contract_id_str: &str,
    validator: &darkfi::validator::Validator,
) -> HandlerResult<ContractId> {
    match contract_id_str {
        "money" => Ok(*darkfi_sdk::crypto::MONEY_CONTRACT_ID),
        "dao" => Ok(*darkfi_sdk::crypto::DAO_CONTRACT_ID),
        "deployooor" => Ok(*darkfi_sdk::crypto::DEPLOYOOOR_CONTRACT_ID),
        _ => {
            // For WASM contracts, we need to look them up
            // This requires scanning the blockchain for the contract
            // For now, return an error indicating the contract needs to be deployed first
            error!(
                target: "contract_registry",
                "WASM contract lookup not yet implemented for: {}",
                contract_id_str
            );
            Err(ContractHandlerError::ContractNotFound(format!(
                "Contract '{}' not found. Native contracts: money, dao, deployooor",
                contract_id_str
            )))
        }
    }
}
