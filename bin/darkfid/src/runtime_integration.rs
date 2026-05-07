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

//! Runtime integration for linear blockchain
//!
//! This module provides a wrapper around darkfi's Runtime for use
//! with LinearBlockchain.

use std::sync::Arc;

use darkfi::{
    runtime::vm_runtime::{Runtime, ContractStoreAccess, SimpleDbAccess, BlockchainAccess},
    Error,
};
use darkfi_sdk::{
    crypto::ContractId,
    tx::TransactionHash,
};
use tracing::{debug, info};

use crate::{blockchain::LinearBlockchain, contract_store::LinearContractStore, linear_simple_db::LinearSimpleDb};

/// Runtime executor for linear blockchain
///
/// Wraps darkfi's Runtime with LinearStore-based adapters.
pub struct LinearRuntime {
    /// Inner darkfi Runtime
    inner: Runtime,
    /// Contract store adapter
    contract_store: Arc<LinearContractStore>,
    /// State db adapter
    state_db: Arc<LinearSimpleDb>,
}

impl LinearRuntime {
    /// Create a new LinearRuntime for executing a contract call
    pub fn new(
        wasm: Vec<u8>,
        contract_id: ContractId,
        blockchain: Arc<LinearBlockchain>,
        verifying_block_height: u32,
        block_target: u32,
        tx_hash: TransactionHash,
        call_idx: u8,
    ) -> Result<Self, Error> {
        let store = blockchain.store.clone();
        let contract_store = Arc::new(LinearContractStore::new(store.clone()));
        let state_db = Arc::new(LinearSimpleDb::new(store));

        info!(
            target: "linear_runtime",
            "Creating LinearRuntime for contract {:?}",
            contract_id
        );

        let inner = Runtime::new(
            &wasm,
            contract_store.clone() as Arc<dyn ContractStoreAccess>,
            state_db.clone() as Arc<dyn SimpleDbAccess>,
            blockchain as Arc<dyn BlockchainAccess>,
            contract_id,
            verifying_block_height,
            block_target,
            tx_hash,
            call_idx,
        )?;

        Ok(Self { inner, contract_store, state_db })
    }

    /// Execute metadata phase
    pub fn metadata(&mut self, payload: &[u8]) -> Result<Vec<u8>, Error> {
        debug!(target: "linear_runtime", "metadata() called");
        self.inner.metadata(payload)
    }

    /// Execute exec phase
    pub fn exec(&mut self, payload: &[u8]) -> Result<Vec<u8>, Error> {
        debug!(target: "linear_runtime", "exec() called");
        self.inner.exec(payload)
    }

    /// Execute apply phase
    pub fn apply(&mut self, state_update: &[u8]) -> Result<(), Error> {
        debug!(target: "linear_runtime", "apply() called");
        self.inner.apply(state_update)
    }

    /// Get gas used by this runtime instance
    pub fn gas_used(&mut self) -> u64 {
        self.inner.gas_used()
    }

    /// Deploy a contract
    pub fn deploy(&mut self, payload: &[u8]) -> Result<(), Error> {
        info!(target: "linear_runtime", "deploy() called for contract");
        self.inner.deploy(payload)
    }
}

/// Execute a transaction on the linear chain
pub async fn execute_tx(
    tx: &darkfi::tx::Transaction,
    blockchain: Arc<LinearBlockchain>,
    verifying_block_height: u32,
    block_target: u32,
) -> Result<(), Error> {
    info!(target: "linear_runtime", "execute_tx() called");

    // For each call in the transaction, execute it
    for (call_idx, call) in tx.calls.iter().enumerate() {
        let contract_id = call.data.contract_id;
        let payload = &call.data.data;

        // Get the WASM for this contract
        let wasm = blockchain.get_contract(contract_id)?;

        let mut runtime = LinearRuntime::new(
            wasm,
            contract_id,
            blockchain.clone(),
            verifying_block_height,
            block_target,
            tx.hash(),
            call_idx as u8,
        )?;

        // Execute metadata phase
        runtime.metadata(payload)?;

        // Execute exec phase
        let result = runtime.exec(payload)?;

        // Execute apply phase with result
        runtime.apply(&result)?;
    }

    Ok(())
}

/// Deploy a new contract to the linear chain
pub fn deploy_contract(
    wasm: &[u8],
    contract_id: ContractId,
    blockchain: Arc<LinearBlockchain>,
    verifying_block_height: u32,
    block_target: u32,
) -> Result<(), Error> {
    info!(target: "linear_runtime", "Deploying contract {:?}", contract_id);

    let mut runtime = LinearRuntime::new(
        wasm.to_vec(),
        contract_id,
        blockchain,
        verifying_block_height,
        block_target,
        TransactionHash::none(),
        0,
    )?;

    runtime.deploy(&[])?;
    info!(target: "linear_runtime", "Contract {:?} deployed successfully", contract_id);
    Ok(())
}