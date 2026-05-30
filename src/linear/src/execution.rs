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

//! WASM contract execution for linear blockchain blocks.
//!
//! Extracts execution jobs from block transactions and runs them against
//! a [`WasmRuntime`]. The runtime is injected via trait so tests can mock it.

use super::{Block, LinearError, Result};

/// A unit of WASM work extracted from a transaction.
#[derive(Debug, Clone)]
pub struct ExecutionJob {
    /// Contract ID to call
    pub contract_id: [u8; 32],
    /// Serialized call data
    pub call_data: Vec<u8>,
    /// Transaction hash this job came from (for deterministic ordering)
    pub tx_hash: [u8; 32],
}

/// Outcome of executing a single [`ExecutionJob`].
#[derive(Debug, Clone)]
pub struct JobResult {
    /// Gas consumed by this call
    pub gas_used: u64,
    /// Key-value pairs written by the contract (key → new value).
    /// A `None` value means the key was deleted.
    pub state_diff: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    /// New contracts deployed during this execution (contract_id → WASM bytes)
    pub new_contracts: Vec<([u8; 32], Vec<u8>)>,
}

/// Trait abstracting WASM contract execution.
///
/// The real implementation uses `dwow_core::Runtime`. Tests inject a mock
/// that returns pre-programmed results without touching actual WASM.
pub trait WasmRuntime: Send + Sync {
    /// Execute a contract call. Returns `(gas_used, state_changes)`.
    fn execute(
        &self,
        contract_id: &[u8; 32],
        call_data: &[u8],
    ) -> std::result::Result<(u64, Vec<(Vec<u8>, Option<Vec<u8>>)>), String>;
}

/// Extract execution jobs from a block's transactions.
///
/// Pure — takes block data, returns a flat list of jobs. Each contract
/// call in each transaction becomes one job.
pub fn build_jobs(block: &Block) -> Vec<ExecutionJob> {
    let mut jobs = Vec::new();
    for tx in &block.transactions {
        let tx_hash = *tx.hash().as_bytes();
        for call in &tx.contract_calls {
            jobs.push(ExecutionJob {
                contract_id: call.contract_id,
                call_data: call.data.clone(),
                tx_hash,
            });
        }
    }
    jobs
}

/// Execute a list of jobs against a [`WasmRuntime`].
///
/// Each job is executed independently. Errors from individual contracts
/// are collected and returned — they do not abort the batch.
pub fn execute_jobs(
    jobs: &[ExecutionJob],
    runtime: &dyn WasmRuntime,
) -> Result<Vec<JobResult>> {
    let mut results = Vec::with_capacity(jobs.len());
    for job in jobs {
        match runtime.execute(&job.contract_id, &job.call_data) {
            Ok((gas_used, state_diff)) => {
                results.push(JobResult {
                    gas_used,
                    state_diff,
                    new_contracts: Vec::new(),
                });
            }
            Err(e) => {
                return Err(LinearError::BlockIsInvalid(format!(
                    "contract {} call failed: {}",
                    hex::encode(job.contract_id),
                    e
                )));
            }
        }
    }
    Ok(results)
}
