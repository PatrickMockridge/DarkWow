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
//! Extracts execution jobs from block transactions, runs them through the
//! WASM runtime, merges per-call state diffs deterministically (canonical
//! first, then uncles with canonical subtraction), and handles deployooor
//! post-processing. Returns a merged [`SledTreeOverlay`] ready for atomic
//! commit.

use std::io::Cursor;
use std::sync::Arc;

use blake3::Hash as Blake3Hash;
use randomx::RandomXVM;
use sled_overlay::{SledTreeOverlay, SledTreeOverlayStateDiff};
use tracing::{error, info};

use dwow_core::Error;
use dwow_serial::Decodable;
use dwow_sdk::crypto::{ContractId, DEPLOYOOOR_CONTRACT_ID};
use dwow_sdk::deploy::DeployParamsV1;

use crate::blockchain::{LinearBlockchain, BLOCK_GAS_LIMIT, TxBackend};

/// Statistics gathered during block execution.
#[derive(Debug, Default)]
pub struct ExecutionStats {
    pub gas_used: u64,
    pub calls_executed: u64,
    pub calls_failed: u64,
    pub uncle_calls_executed: u64,
    pub uncle_calls_failed: u64,
}

/// The merged overlay after all contract calls execute, plus any
/// pending deployooor deployments to process.
pub struct ExecutionOutcome {
    pub overlay: SledTreeOverlay,
    pub stats: ExecutionStats,
}

/// Execute all contract calls in a block and its uncles against the
/// blockchain's stored contracts.
///
/// Returns the merged [`SledTreeOverlay`] containing all state changes,
/// ready for atomic commit. Deployooor deployments are handled inline —
/// their WASM bytes are written through the overlay and `__initialize`
/// is called within the same overlay context.
pub fn execute_block(
    blockchain: &LinearBlockchain,
    block: &dwow_chain::Block,
    uncles: &[dwow_chain::UncleBlock],
    vm: &Arc<RandomXVM>,
    current_height: u64,
    difficulty: u32,
) -> dwow_core::Result<ExecutionOutcome> {
    let store = blockchain.store.clone();
    let contracts_tree = store.contracts_tree().clone();
    let base_overlay = SledTreeOverlay::new(&contracts_tree);
    let mut early_fail = 0u64;

    // ---- Build execution jobs ----

    struct CallJob {
        tx_hash: Blake3Hash,
        is_canonical: bool,
        overlay: SledTreeOverlay,
        wasm_bytes: Vec<u8>,
        contract_id: ContractId,
        call_data: Vec<u8>,
        call_idx: u8,
    }

    let mut jobs: Vec<CallJob> = Vec::new();

    // Canonical transactions
    for tx in &block.transactions {
        let tx_hash = tx.hash();
        for (call_idx, call) in tx.contract_calls.iter().enumerate() {
            let contract_id = match ContractId::from_bytes(call.contract_id) {
                Ok(cid) => cid,
                Err(_) => { early_fail += 1; continue; }
            };
            let wasm_bytes = match store.get_contract_data(&call.contract_id) {
                Ok(b) => b,
                Err(_) => { early_fail += 1; continue; }
            };
            if wasm_bytes.is_empty() { early_fail += 1; continue; }
            jobs.push(CallJob {
                tx_hash,
                is_canonical: true,
                overlay: base_overlay.clone(),
                wasm_bytes,
                contract_id,
                call_data: call.data.clone(),
                call_idx: call_idx as u8,
            });
        }
    }

    // Uncle transactions
    for uncle in uncles.iter() {
        for tx in &uncle.transactions {
            let tx_hash = tx.hash();
            for (call_idx, call) in tx.contract_calls.iter().enumerate() {
                let contract_id = match ContractId::from_bytes(call.contract_id) {
                    Ok(cid) => cid,
                    Err(_) => { early_fail += 1; continue; }
                };
                let wasm_bytes = match store.get_contract_data(&call.contract_id) {
                    Ok(b) => b,
                    Err(_) => { early_fail += 1; continue; }
                };
                if wasm_bytes.is_empty() { early_fail += 1; continue; }
                jobs.push(CallJob {
                    tx_hash,
                    is_canonical: false,
                    overlay: base_overlay.clone(),
                    wasm_bytes,
                    contract_id,
                    call_data: call.data.clone(),
                    call_idx: call_idx as u8,
                });
            }
        }
    }

    // ---- Execute all calls sequentially ----

    struct CallResult {
        tx_hash: Blake3Hash,
        is_canonical: bool,
        success: bool,
        gas: u64,
        diff: Option<SledTreeOverlayStateDiff>,
    }

    let mut results: Vec<CallResult> = Vec::with_capacity(jobs.len());
    let mut pending_deployments: Vec<(Vec<u8>, ContractId, Vec<u8>)> = Vec::new();

    for job in jobs {
        let is_canonical = job.is_canonical;
        let tx_hash = job.tx_hash;
        let backend = Arc::new(TxBackend {
            overlay: std::sync::Mutex::new(job.overlay),
            store: store.clone(),
            height: current_height,
            vm: vm.clone(),
        });

        backend.overlay.lock().unwrap().checkpoint();

        let tx_hash_bytes = dwow_sdk::tx::TransactionHash(*tx_hash.as_bytes());
        let mut runtime = match dwow_core::runtime::vm_runtime::Runtime::new(
            &job.wasm_bytes,
            backend.clone(),
            job.contract_id,
            current_height as u32,
            difficulty,
            tx_hash_bytes,
            job.call_idx,
        ) {
            Ok(r) => r,
            Err(_) => {
                backend.overlay.lock().unwrap().revert_to_checkpoint();
                results.push(CallResult { tx_hash, is_canonical, success: false, gas: 0, diff: None });
                continue;
            }
        };

        let mut success = true;
        if runtime.metadata(&job.call_data).is_err() { success = false; }
        if success && runtime.exec(&job.call_data).is_err() { success = false; }

        // Spend hook callback dispatch
        if success {
            let hook_request = runtime.ctx.as_ref(&runtime.store).spend_hook_request.take();
            if let Some((target_cid_bytes, payload)) = hook_request {
                success = execute_spend_hook(
                    &store, backend.clone(), &target_cid_bytes, &payload,
                    current_height, difficulty, tx_hash_bytes,
                );
            }
        }

        if success && runtime.apply(&[]).is_err() { success = false; }

        if !success {
            backend.overlay.lock().unwrap().revert_to_checkpoint();
            results.push(CallResult { tx_hash, is_canonical, success: false, gas: 0, diff: None });
            continue;
        }

        let gas = runtime.gas_used();
        let diff = SledTreeOverlayStateDiff::new(
            &contracts_tree,
            &backend.overlay.lock().unwrap().state,
        ).ok();
        results.push(CallResult { tx_hash, is_canonical, success: true, gas, diff });

        // Deployooor post-processing
        if is_canonical && job.contract_id == *DEPLOYOOOR_CONTRACT_ID {
            if let Ok(calls) = dwow_serial::deserialize::<Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::tx::ContractCall>>>(&job.call_data) {
                if (job.call_idx as usize) < calls.len() {
                    let inner = &calls[job.call_idx as usize].data;
                    if inner.data.len() > 1 && inner.data[0] == 0x00 {
                        if let Ok(params) = DeployParamsV1::decode(&mut Cursor::new(&inner.data[1..])) {
                            let deployed_id = ContractId::derive_public(params.public_key);
                            pending_deployments.push((params.wasm_bincode, deployed_id, params.ix));
                        }
                    }
                }
            }
        }
    }

    // ---- Merge results in deterministic order (canonical-first) ----

    results.sort_by(|a, b| a.tx_hash.as_bytes().cmp(b.tx_hash.as_bytes()));

    let mut main_overlay = base_overlay;
    let mut cumulative_gas: u64 = 0;
    let mut calls_executed = 0u64;
    let mut calls_failed = early_fail;
    let mut uncle_calls_executed = 0u64;
    let mut uncle_calls_failed = 0u64;

    for r in results.iter().filter(|r| r.is_canonical) {
        if r.success {
            calls_executed += 1;
            cumulative_gas += r.gas;
            if cumulative_gas >= BLOCK_GAS_LIMIT {
                return Err(Error::Custom("BlockGasLimitExceeded".to_string()));
            }
            if let Some(ref diff) = r.diff { main_overlay.add_diff(diff); }
        } else {
            calls_failed += 1;
        }
    }

    let canonical_total = SledTreeOverlayStateDiff::new(
        &contracts_tree, &main_overlay.state,
    ).map_err(|e| Error::Custom(e.to_string()))?;

    for r in results.iter().filter(|r| !r.is_canonical) {
        if r.success {
            uncle_calls_executed += 1;
            cumulative_gas += r.gas;
            if cumulative_gas >= BLOCK_GAS_LIMIT {
                return Err(Error::Custom("BlockGasLimitExceeded".to_string()));
            }
            let mut diff = r.diff.clone();
            if let Some(ref mut d) = diff {
                d.remove_diff(&canonical_total);
                main_overlay.add_diff(d);
            }
        } else {
            uncle_calls_failed += 1;
        }
    }

    if calls_failed > 0 {
        error!(target: "execution", "Block had {} failed calls out of {}",
            calls_failed, calls_executed + calls_failed);
    } else if calls_executed > 0 {
        info!(target: "execution", "Block executed {} calls successfully", calls_executed);
    }

    // ---- Deployooor post-processing ----
    for (wasm, contract_id, ix) in &pending_deployments {
        info!(target: "execution", "Deploying {:?} via Deployooor", contract_id);
        deploy_contract_in_overlay(
            &mut main_overlay, (*store).clone(), vm.clone(),
            wasm, *contract_id, ix, current_height, difficulty,
        )?;
    }

    Ok(ExecutionOutcome {
        overlay: main_overlay,
        stats: ExecutionStats {
            gas_used: cumulative_gas,
            calls_executed,
            calls_failed,
            uncle_calls_executed,
            uncle_calls_failed,
        },
    })
}

/// Execute a spend-hook callback to another contract within the same overlay.
fn execute_spend_hook(
    store: &dwow_chain::LinearStore,
    backend: Arc<TxBackend>,
    target_cid_bytes: &[u8],
    payload: &[u8],
    current_height: u64,
    difficulty: u32,
    tx_hash_bytes: dwow_sdk::tx::TransactionHash,
) -> bool {
    let target_wasm_bytes = match store.get_contract_data(target_cid_bytes) {
        Ok(b) if !b.is_empty() => b,
        _ => { error!(target: "execution", "spend_hook: target contract not found"); return false; }
    };

    let cid_arr: [u8; 32] = target_cid_bytes[0..32].try_into().unwrap();
    let target_cid = match ContractId::from_bytes(cid_arr) {
        Ok(c) => c,
        Err(_) => { error!(target: "execution", "spend_hook: invalid target CID"); return false; }
    };

    let mut target_runtime = match dwow_core::runtime::vm_runtime::Runtime::new(
        &target_wasm_bytes, backend.clone(), target_cid,
        current_height as u32, difficulty, tx_hash_bytes, 0u8,
    ) {
        Ok(r) => r,
        Err(_) => { error!(target: "execution", "spend_hook: runtime creation failed"); return false; }
    };

    if target_runtime.metadata(payload).is_err() { return false; }
    let ret = match target_runtime.spend_hook(payload) {
        Ok(r) => r,
        Err(_) => { return false; }
    };
    target_runtime.apply(&ret).is_ok()
}

/// Deploy a contract within an overlay (for Deployooor post-processing).
fn deploy_contract_in_overlay(
    overlay: &mut SledTreeOverlay,
    store: dwow_chain::LinearStore,
    vm: Arc<RandomXVM>,
    wasm: &[u8],
    contract_id: ContractId,
    ix: &[u8],
    current_height: u64,
    difficulty: u32,
) -> dwow_core::Result<()> {
    overlay.state.cache.insert(
        sled_overlay::sled::IVec::from(contract_id.to_bytes().as_slice()),
        sled_overlay::sled::IVec::from(wasm),
    );

    let deploy_overlay = std::sync::Mutex::new(overlay.clone());
    let backend = Arc::new(TxBackend {
        overlay: deploy_overlay,
        store: Arc::new(store),
        height: current_height,
        vm,
    });
    let mut runtime = dwow_core::runtime::vm_runtime::Runtime::new(
        wasm, backend.clone(), contract_id, current_height as u32,
        difficulty, dwow_sdk::tx::TransactionHash::none(), 0,
    ).map_err(|e| Error::Custom(format!("DeployV1 runtime: {}", e)))?;
    runtime.deploy(ix).map_err(|e| Error::Custom(format!("DeployV1 init: {}", e)))?;
    drop(runtime);

    let deploy_overlay = Arc::try_unwrap(backend)
        .map_err(|_| Error::Custom("backend still referenced after deploy".to_string()))?
        .overlay.into_inner().unwrap();
    *overlay = deploy_overlay;
    Ok(())
}
