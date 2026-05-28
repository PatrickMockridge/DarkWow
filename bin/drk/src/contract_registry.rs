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

//! Generic Contract Registry for drk Wallet
//!
//! This module provides a generic system for handling contracts with dependencies.
//! Contracts like stablecoin, dex, and lottery depend on promissory_note for token transfers.

use dwow_sdk::crypto::ContractId;
use dwow_sdk::pasta::pallas;

// ============================================================================
// Contract Trait and Registry
// ============================================================================

/// Trait for contracts that can be used in transactions
pub trait Contract: Send + Sync {
    /// Unique identifier for this contract
    fn contract_id(&self) -> ContractId;

    /// Human-readable name
    fn name(&self) -> &'static str;

    /// Direct dependencies (other contract IDs this contract requires)
    fn dependencies(&self) -> Vec<ContractId>;

    /// Check if this contract is initialized (runtime registration complete)
    fn is_initialized(&self) -> bool;
}

/// Registry of all available contracts
///
/// Note: Uses Vec for simplicity since ContractId only implements Eq, not Hash.
/// For production, consider using a BTreeMap with custom Comparator.
pub struct ContractRegistry {
    contracts: Vec<Box<dyn Contract>>,
}

impl ContractRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self { contracts: Vec::new() }
    }

    /// Register a new contract
    pub fn register(&mut self, contract: Box<dyn Contract>) {
        self.contracts.push(contract);
    }

    /// Get contract by ID
    pub fn get(&self, id: &ContractId) -> Option<&Box<dyn Contract>> {
        self.contracts.iter().find(|c| c.contract_id() == *id)
    }

    /// Check if a contract is registered
    pub fn is_registered(&self, id: &ContractId) -> bool {
        self.contracts.iter().any(|c| c.contract_id() == *id)
    }

    /// Resolve all dependencies for a contract (transitive closure)
    pub fn resolve_dependencies(&self, id: &ContractId) -> Vec<ContractId> {
        let mut deps = Vec::new();
        let mut visited = Vec::new();
        self.resolve_deps_recursive(id, &mut deps, &mut visited);
        deps
    }

    fn resolve_deps_recursive(
        &self,
        id: &ContractId,
        deps: &mut Vec<ContractId>,
        visited: &mut Vec<ContractId>,
    ) {
        if visited.contains(id) {
            return;
        }
        visited.push(*id);

        if let Some(contract) = self.get(id) {
            for dep_id in contract.dependencies() {
                if !visited.contains(&dep_id) {
                    deps.push(dep_id);
                    self.resolve_deps_recursive(&dep_id, deps, visited);
                }
            }
        }
    }

    /// Check if all dependencies of a contract are available and initialized
    pub fn can_instantiate(&self, id: &ContractId) -> bool {
        if let Some(contract) = self.get(id) {
            if !contract.is_initialized() {
                return false;
            }
            for dep_id in contract.dependencies() {
                if !self.can_instantiate(&dep_id) {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }

    /// Get all registered contracts
    pub fn contracts(&self) -> &[Box<dyn Contract>] {
        &self.contracts
    }
}

impl Default for ContractRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Contract Call Tree (for child call handling)
// ============================================================================

/// Represents a contract call with its child dependencies
#[derive(Debug, Clone)]
pub struct ContractCallTree {
    /// The contract call
    pub call: dwow_sdk::tx::ContractCall,
    /// Child calls that must be executed with this call
    pub children: Vec<ContractCallTree>,
}

/// Create a spend_hook child call if coin has non-zero spend_hook
///
/// When a transfer output coin has a non-zero spend_hook, this function creates
/// a child ContractCallTree that will be executed after the transfer completes.
pub fn create_spend_hook_call(
    spend_hook: pallas::Base,
    _user_data: pallas::Base,
) -> Option<ContractCallTree> {
    if spend_hook == pallas::Base::zero() {
        return None;
    }

    // The spend_hook is a ContractId stored as pallas::Base
    let hook_contract_id = ContractId::from(spend_hook);

    // Create a placeholder transfer call data
    // The actual params would be populated from the coin's user_data
    let call_data = vec![0x04u8]; // TransferV1 function code

    let transfer_call = dwow_sdk::tx::ContractCall {
        contract_id: hook_contract_id,
        data: call_data,
    };

    Some(ContractCallTree { call: transfer_call, children: vec![] })
}
