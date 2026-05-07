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

//! Chain Registry
//!
//! Plugin registry for chain handlers. The registry maps `ChainId`
//! to handler implementations.
//!
//! ## Usage
//!
//! ```ignore
//! let registry = ChainRegistry::new();
//! let handler = registry.get(ChainId::Ethereum)?;
//! let verified = handler.verify_deposit(&deposit).await?;
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use dwow_sdk::error::ContractError;

use super::{ChainHandler, ChainId};

/// Registry of chain handlers
///
/// Maps `ChainId` to handler implementations.
/// Handlers are registered at initialization and accessed by chain ID.
pub struct ChainRegistry {
    /// Map from chain ID to handler
    handlers: HashMap<ChainId, Arc<dyn ChainHandler>>,
}

impl ChainRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self { handlers: HashMap::new() }
    }

    /// Register a handler for a chain
    ///
    /// # Panics
    ///
    /// Panics if a handler is already registered for this chain
    pub fn register<H: ChainHandler + 'static>(&mut self, handler: H) {
        let chain_id = handler.chain_id();
        if self.handlers.contains_key(&chain_id) {
            panic!("Handler already registered for chain: {:?}", chain_id);
        }
        self.handlers.insert(chain_id, Arc::new(handler));
    }

    /// Get a handler for a chain
    pub fn get(&self, chain_id: ChainId) -> Result<Arc<dyn ChainHandler>, ContractError> {
        self.handlers
            .get(&chain_id)
            .cloned()
            .ok_or_else(|| ContractError::Custom(1))
    }

    /// Check if a handler exists for a chain
    pub fn contains(&self, chain_id: ChainId) -> bool {
        self.handlers.contains_key(&chain_id)
    }

    /// Get all registered chain IDs
    pub fn registered_chains(&self) -> Vec<ChainId> {
        self.handlers.keys().cloned().collect()
    }

    /// Get count of registered handlers
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

impl Default for ChainRegistry {
    fn default() -> Self {
        Self::new()
    }
}
