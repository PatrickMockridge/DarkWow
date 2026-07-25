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
//! Extracts execution jobs from block transactions and runs them through the
//! WASM runtime. Canonical calls execute sequentially in block order against
//! a single shared overlay (layer-2 sequential visibility — see consensus.md
//! "Execution Ordering & Atomicity Layers"); any canonical failure rejects
//! the block. Uncle calls execute against isolated pre-block clones and merge
//! canonical-wins. Handles deployooor post-processing. Returns the canonical
//! [`SledTreeOverlay`] ready for atomic commit.
//!
//! # Proof of Token Balance
//!
//! The proof of token balance (`crate::proof_of_token_balance`) is now an
//! **active consensus rule** enforced at every block acceptance path. It
//! combines the cumulative supply chain (`S_H = S_{H-1} + C_H`) with a
//! per-block Pedersen mass balance equation (`Σ outputs + Σ burns + Σ fees
//! == Σ inputs`). Blocks that fail the check are rejected before chain
//! application.
//!
//! This module (`execute_block`) provides WASM-level execution and is the
//! next layer of defense — wiring it into [`connect_block`] would add ZK
//! proof verification at block validation time. The mass balance check
//! already covers the supply conservation invariant without requiring WASM
//! execution at the chain level.
//!
//! [`connect_block`]: dwow_linear::chain_state::CChainState::connect_block
//! [`verify_cumulative_supply`]: dwow_sdk::blockchain::verify_cumulative_supply

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::Arc;

use blake3::Hash as Blake3Hash;
use randomx::RandomXVM;
use sled_overlay::{SledTreeOverlay, SledTreeOverlayState, SledTreeOverlayStateDiff};
use tracing::{error, info};

use dwow_core::Error;
use dwow_serial::Decodable;
use dwow_sdk::blockchain::{BlockHeight, BlockTarget};
use dwow_sdk::crypto::{
    ContractId, ATTESTATION_CONTRACT_ID, BOX_CONTRACT_ID, DEPLOYOOOR_CONTRACT_ID,
    IDENTITY_CONTRACT_ID, MULTISIG_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID, ORACLE_CONTRACT_ID,
    PROMISSORY_NOTE_CONTRACT_ID, PURSE_CONTRACT_ID,
};
use dwow_sdk::deploy::DeployParamsV1;

use crate::CChainState;
use crate::LinearStore;
use crate::schedule::ExecutionSchedule;
use dwow_core::runtime::vm_runtime::RuntimeBackend;

/// Maximum gas a single block can consume across all contract calls.
/// Formerly in the deleted `blockchain.rs` god object.
pub const BLOCK_GAS_LIMIT: u64 = 100_000_000_000;

/// Maximum serialized size (bytes) of a block on the P2P wire and on disk.
/// Single source of truth pinned across nodes (L1 barrier #7): the block-wire
/// decoder fails closed at this bound, and the miner's template must never
/// exceed it — otherwise a self-built block is rejected by every peer. Proof
/// witnesses now ride inside each transaction, so byte accounting is required
/// in addition to gas accounting.
pub const MAX_BLOCK_SIZE: usize = 4 * 1024 * 1024;

/// WASM runtime backend providing sled overlay access for contract execution.
///
/// Execution ordering per consensus.md "Execution Ordering & Atomicity Layers":
/// canonical calls share ONE overlay and execute sequentially in block order —
/// call N observes all writes of calls 1..N-1 (layer-2 sequential visibility).
/// Per-call `checkpoint()`/`revert_to_checkpoint()` provides layer-1
/// transaction atomicity: a failing call leaves zero writes. The hybrid read
/// pattern (overlay-get first, then bare-tree fallback via
/// `store.get_contract_data`) additionally provides visibility into data
/// written by PRIOR blocks. Uncle calls execute against independent clones of
/// the pre-block state (they were mined against it) and merge canonical-wins.
pub struct TxBackend {
    pub overlay: std::sync::Arc<std::sync::Mutex<sled_overlay::SledTreeOverlay>>,
    pub store: std::sync::Arc<LinearStore>,
    pub height: BlockHeight,
    pub vm: std::sync::Arc<randomx::RandomXVM>,
}

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
    chain_state: &CChainState,
    block: &crate::Block,
    uncles: &[crate::UncleBlock],
    vm: &Arc<RandomXVM>,
    current_height: BlockHeight,
    difficulty: BlockTarget,
) -> dwow_core::Result<ExecutionOutcome> {
    let store = chain_state.store.clone();
    let contracts_tree = store.contracts_tree().clone();
    let base_overlay = SledTreeOverlay::new(&contracts_tree);
    let mut early_fail = 0u64;

    // Layer-2 (merkle-tree atomicity): ONE shared overlay for all canonical
    // calls, executed sequentially in block order. Call N observes writes of
    // calls 1..N-1 — required by fee_collect_v1 reading fees_db[H] accumulated
    // by this block's FeeV1 calls, and by every coin-creating call appending
    // to the same coin merkle tree. See consensus.md "Execution Ordering &
    // Atomicity Layers".
    let canonical_overlay = Arc::new(std::sync::Mutex::new(SledTreeOverlay::new(&contracts_tree)));

    // ---- Genesis-deployment pre-pass (hard-coded genesis rule) ----
    // At height GENESIS the block carries the 9 contract deployments as
    // transactions. They are materialized into the canonical overlay BEFORE
    // any call executes — the coinbase's NativeToken call resolves against
    // the overlay (lookup below), and everything commits in the block's
    // single atomic batch. Identical for locally-created and synced genesis.
    let is_genesis = block.header.height == BlockHeight::GENESIS;
    if is_genesis {
        let mut guard = canonical_overlay.lock().unwrap_or_else(|e| e.into_inner());
        apply_genesis_deployments(&mut guard, (*store).clone(), Arc::clone(vm), block, difficulty)?;
    }

    // ---- Build execution jobs ----

    struct CallJob {
        tx_hash: Blake3Hash,
        is_canonical: bool,
        overlay: Arc<std::sync::Mutex<SledTreeOverlay>>,
        wasm_bytes: Vec<u8>,
        contract_id: ContractId,
        call_data: Vec<u8>,
        call_idx: u8,
    }

    let mut jobs: Vec<CallJob> = Vec::new();

    // Canonical transactions — in block order (transaction order, call order
    // within transaction). Missing/empty contract WASM is a hard reject in
    // strict mode: a canonical call that cannot execute would silently skip
    // state transitions the block header commits to.
    for tx in &block.transactions {
        // Genesis deployment txs were consumed by the pre-pass above — they
        // MUST NOT dispatch through Deployooor WASM (Deployooor itself is
        // one of the payloads).
        if is_genesis && is_genesis_deployment_tx(tx) {
            continue;
        }
        let tx_hash = tx.hash();
        for (call_idx, call) in tx.contract_calls.iter().enumerate() {
            // Phase 2.1: contract_id is now typed ContractId — no from_bytes needed
            let contract_id = call.contract_id;
            // At genesis the contracts were just deployed into the canonical
            // overlay and are not yet committed — resolve overlay-first.
            // Non-genesis heights keep the committed-state read untouched.
            let overlay_wasm: Option<Vec<u8>> = if is_genesis {
                let guard = canonical_overlay.lock().unwrap_or_else(|e| e.into_inner());
                match guard.get(&contract_id.to_bytes()) {
                    Ok(Some(iv)) if !iv.is_empty() => Some(iv.to_vec()),
                    _ => None,
                }
            } else {
                None
            };
            let wasm_bytes = match overlay_wasm {
                Some(b) => b,
                None => match store.get_contract_data(&contract_id.to_bytes()) {
                    Ok(b) if !b.is_empty() => b,
                    _ => {
                        return Err(Error::Custom(format!(
                            "canonical call {} of tx {} targets unknown contract {}",
                            call_idx, hex::encode(tx_hash.as_bytes()), contract_id,
                        )));
                    }
                },
            };
            jobs.push(CallJob {
                tx_hash,
                is_canonical: true,
                overlay: Arc::clone(&canonical_overlay),
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
                // Phase 2.1: contract_id is now typed ContractId
                let contract_id = call.contract_id;
                let wasm_bytes = match store.get_contract_data(&contract_id.to_bytes()) {
                    Ok(b) => b,
                    Err(_) => { early_fail += 1; continue; }
                };
                if wasm_bytes.is_empty() { early_fail += 1; continue; }
                jobs.push(CallJob {
                    tx_hash,
                    is_canonical: false,
                    overlay: Arc::new(std::sync::Mutex::new(base_overlay.clone())),
                    wasm_bytes,
                    contract_id,
                    call_data: call.data.clone(),
                    call_idx: call_idx as u8,
                });
            }
        }
    }

    // ---- Execute all calls sequentially ----
    //
    // Canonical calls are inherently sequential per consensus.md "Execution
    // Ordering & Atomicity Layers" — call N observes writes of calls 1..N-1
    // on the shared overlay. Only uncle calls (isolated pre-block clones)
    // could ever parallelize (Tier 6, wasmer-gated).

    // Uncle call results — canonical calls execute strictly and never
    // produce a tolerated-failure result.
    struct CallResult {
        success: bool,
        gas: u64,
        diff: Option<SledTreeOverlayStateDiff>,
    }

    let mut uncle_results: Vec<CallResult> = Vec::new();
    let mut pending_deployments: Vec<(Vec<u8>, ContractId, Vec<u8>)> = Vec::new();

    // L2 verification state: accumulate per-tx metadata tables (zkp + pub)
    // during the job loop, then verify proofs+sigs after the loop.  Keyed by
    // canonical tx_hash so only on-chain txs are verified (uncles excluded).
    struct TxVeriTables {
        zkp: Vec<Vec<(String, Vec<dwow_sdk::pasta::pallas::Base>)>>,
        pubkeys: Vec<Vec<dwow_sdk::crypto::PublicKey>>,
    }
    let mut veri_state: HashMap<Blake3Hash, TxVeriTables> = HashMap::new();

    let mut cumulative_gas: u64 = 0;
    let mut calls_executed = 0u64;
    let mut uncle_calls_executed = 0u64;
    let mut uncle_calls_failed = 0u64;

    // Per-call write key sets for the ExecutionSchedule diagnostic (§9.4).
    // Canonical keys are computed by before/after snapshot on the shared
    // overlay (same pattern as Deployooor post-processing below).
    let mut per_call_keys: Vec<HashSet<Vec<u8>>> = Vec::with_capacity(jobs.len());

    for job in jobs {
        let is_canonical = job.is_canonical;
        let tx_hash = job.tx_hash;
        let backend = Arc::new(TxBackend {
            overlay: Arc::clone(&job.overlay),
            store: store.clone(),
            height: current_height,
            vm: vm.clone(),
        });

        // Layer-1 (transaction atomicity): checkpoint before the call; any
        // failure reverts to it, leaving zero writes from this call.
        backend.overlay.lock().unwrap_or_else(|e| e.into_inner()).checkpoint();

        // Diagnostic snapshot — canonical write-set = after-keys minus before-keys.
        let before_keys: Option<HashSet<Vec<u8>>> = if is_canonical {
            let guard = backend.overlay.lock().unwrap_or_else(|e| e.into_inner());
            Some(guard.state.cache.keys().chain(guard.state.removed.iter()).map(|k| k.to_vec()).collect())
        } else {
            None
        };

        let tx_hash_bytes = dwow_sdk::tx::TransactionHash(*tx_hash.as_bytes());
        let mut runtime = match dwow_core::runtime::vm_runtime::Runtime::new(
            &job.wasm_bytes,
            backend.clone(),
            job.contract_id,
            current_height,
            difficulty,
            tx_hash_bytes,
            job.call_idx,
        ) {
            Ok(r) => r,
            Err(e) => {
                backend.overlay.lock().unwrap_or_else(|e| e.into_inner()).revert_to_checkpoint();
                if is_canonical {
                    // Strict mode: a canonical call that cannot execute rejects
                    // the block (consensus.md Execution Ordering, failure semantics).
                    return Err(Error::Custom(format!(
                        "canonical call runtime creation failed for tx {}: {}",
                        hex::encode(tx_hash.as_bytes()), e,
                    )));
                }
                uncle_results.push(CallResult { success: false, gas: 0, diff: None });
                continue;
            }
        };

        let mut success = true;
        let mut fail_stage = "";
        // L2: capture the `metadata()` return — the contract's declared ZK
        // public inputs + signature pubkeys — instead of discarding it.
        // Accumulate per-tx for post-loop verification with the witness.
        match runtime.metadata(&job.call_data) {
            Ok(m) => {
                let mut c = Cursor::new(&m);
                let zkp: Vec<(String, Vec<dwow_sdk::pasta::pallas::Base>)> =
                    match dwow_serial::Decodable::decode(&mut c) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::error!(target: "dwow_chain::execution",
                                "metadata ZK public inputs decode failed: {:?}", e);
                            continue;
                        }
                    };
                let sigs: Vec<dwow_sdk::crypto::PublicKey> =
                    match dwow_serial::Decodable::decode(&mut c) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::error!(target: "dwow_chain::execution",
                                "metadata signature pubkeys decode failed: {:?}", e);
                            continue;
                        }
                    };
                let entry = veri_state.entry(job.tx_hash).or_insert_with(|| {
                    TxVeriTables { zkp: vec![], pubkeys: vec![] }
                });
                entry.zkp.push(zkp);
                entry.pubkeys.push(sigs);
            }
            Err(e) => {
                success = false;
                fail_stage = "metadata";
                eprintln!("metadata() failed for contract {} (tx {}) fn_code={:?}: {:?}",
                    job.contract_id, job.tx_hash, job.call_data.first(), e);
            }
        }

        // exec() computes the state update and returns it as `[selector | update]`.
        // The host MUST thread those bytes into apply() below (see spend-hook path
        // in execute_spend_hook: `apply(&ret)`). Dropping them and calling apply(&[])
        // makes the contract's __update deserialize an empty slice and panic.
        let mut update = Vec::new();
        if success {
            match runtime.exec(&job.call_data) {
                Ok(u) => update = u,
                Err(e) => {
                    eprintln!("exec() failed for contract {} (tx {}): {:?}",
                        job.contract_id, job.tx_hash, e);
                    success = false; fail_stage = "exec";
                }
            }
        }

        // Spend hook callback dispatch
        if success {
            let hook_request = runtime.ctx.as_ref(&runtime.store).spend_hook_request.take();
            if let Some((target_cid_bytes, payload)) = hook_request {
                success = execute_spend_hook(
                    &store, backend.clone(), &target_cid_bytes, &payload,
                    current_height, difficulty, tx_hash_bytes,
                );
                if !success { fail_stage = "spend_hook"; }
            }
        }

        if success {
            if let Err(e) = runtime.apply(&update) {
                eprintln!("apply() failed for contract {} (tx {}): {:?}",
                    job.contract_id, job.tx_hash, e);
                success = false; fail_stage = "apply";
            }
        }

        if !success {
            backend.overlay.lock().unwrap_or_else(|e| e.into_inner()).revert_to_checkpoint();
            if is_canonical {
                // Strict mode: any failed canonical call rejects the block.
                // A valid miner never includes failing txs — mempool admission
                // verifies witnesses, and the miner re-executes at assembly.
                return Err(Error::Custom(format!(
                    "canonical call failed at {} for tx {} (contract {})",
                    fail_stage, hex::encode(tx_hash.as_bytes()), job.contract_id,
                )));
            }
            uncle_results.push(CallResult { success: false, gas: 0, diff: None });
            continue;
        }

        let gas = runtime.gas_used();

        if is_canonical {
            calls_executed += 1;
            cumulative_gas += gas;
            if cumulative_gas >= BLOCK_GAS_LIMIT {
                return Err(Error::Custom("BlockGasLimitExceeded".to_string()));
            }
            // Diagnostic: this call's write-set (after minus before).
            if let Some(before) = before_keys {
                let guard = backend.overlay.lock().unwrap_or_else(|e| e.into_inner());
                let call_keys: HashSet<Vec<u8>> = guard.state.cache.keys()
                    .chain(guard.state.removed.iter())
                    .map(|k| k.to_vec())
                    .filter(|k| !before.contains(k))
                    .collect();
                drop(guard);
                per_call_keys.push(call_keys);
            }
        } else {
            let diff = SledTreeOverlayStateDiff::new(
                &contracts_tree,
                &backend.overlay.lock().unwrap_or_else(|e| e.into_inner()).state,
            ).ok();
            uncle_results.push(CallResult { success: true, gas, diff });
        }

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

    // ---- L2: verify every canonical non-coinbase tx's witness ----
    // Uses the metadata tables accumulated during the job loop above.
    // A fabricated tx (HAZOP C1 — no valid proof) is rejected here,
    // BEFORE any state changes are merged.
    for tx in &block.transactions {
        let is_coinbase =
            tx.contract_calls.first().map_or(false, |c| c.data.first() == Some(&0x05));
        if is_coinbase {
            continue;
        }
        // Genesis deployment txs carry no witness or metadata — their
        // authenticity is the pinned genesis hash + merkle root (verified
        // by check_block_header on every acceptance path).
        if is_genesis && is_genesis_deployment_tx(tx) {
            continue;
        }
        let tx_hash = tx.hash();
        if let Some(tables) = veri_state.get(&tx_hash) {
            let core_tx = crate::zk_verifier::decode_and_reconcile(tx).map_err(|e| {
                dwow_core::Error::Custom(format!(
                    "L2 witness verify tx {}: {}",
                    hex::encode(tx_hash.as_bytes()),
                    e,
                ))
            })?;
            crate::zk_verifier::verify_core_tx_with_tables(
                &store,
                &core_tx,
                &tables.zkp,
                &tables.pubkeys,
            )
            .map_err(|e| {
                dwow_core::Error::Custom(format!(
                    "L2 proof/signature verify tx {}: {}",
                    hex::encode(tx_hash.as_bytes()),
                    e,
                ))
            })?;
        } else {
            // The tx has contract calls but no metadata was accumulated —
            // this should not happen in normal execution (every call runs
            // metadata).  Treat as a verification gap and reject.
            return Err(dwow_core::Error::Custom(format!(
                "L2 verify: tx {} has no accumulated metadata tables",
                hex::encode(tx_hash.as_bytes()),
            )));
        }
    }

    // ---- Canonical overlay becomes the main overlay ----
    // Layer-2 complete: the shared overlay already holds every canonical
    // write in block order — no per-call diff merge, no cross-call conflict
    // resolution needed (sequential visibility supersedes the former
    // same-block double-write rejection for canonical calls; intra-block
    // double-spends fail at the entrypoint SMT check instead).
    let mut main_overlay =
        canonical_overlay.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let calls_failed = early_fail;

    let canonical_total = SledTreeOverlayStateDiff::new(
        &contracts_tree, &main_overlay.state,
    ).map_err(|e| Error::Custom(e.to_string()))?;

    // Seed written_keys with the canonical write-set — Deployooor conflict
    // detection below checks deployments against already-written state.
    let mut written_keys: HashSet<Vec<u8>> = {
        let state = SledTreeOverlayState::from(&canonical_total);
        state.cache.keys().chain(state.removed.iter()).map(|k| k.to_vec()).collect()
    };

    // ---- Merge uncle results (canonical-wins) ----
    // Track uncle-written keys for duplicate-key conflict detection.
    // Uncle-vs-canonical conflicts are resolved by remove_diff (canonical wins).
    // Uncle-vs-uncle conflicts MUST be rejected — two uncles writing the same
    // key would produce non-deterministic results dependent on iteration order.
    let mut uncle_written_keys: HashSet<Vec<u8>> = HashSet::new();

    for r in uncle_results.iter() {
        if r.success {
            uncle_calls_executed += 1;
            cumulative_gas += r.gas;
            if cumulative_gas >= BLOCK_GAS_LIMIT {
                return Err(Error::Custom("BlockGasLimitExceeded".to_string()));
            }
            let mut diff = r.diff.clone();
            if let Some(ref mut d) = diff {
                // Subtract canonical writes (canonical wins on conflict)
                d.remove_diff(&canonical_total);
                // Reject uncle-vs-uncle duplicate key writes.
                let overlay_state = SledTreeOverlayState::from(&*d);
                for key in overlay_state.cache.keys().chain(overlay_state.removed.iter()) {
                    if !uncle_written_keys.insert(key.to_vec()) {
                        return Err(Error::Custom(
                            "DuplicateKeyConflict: two uncles wrote the same key".to_string()
                        ));
                    }
                }
                main_overlay.add_diff(d);
            }
        } else {
            uncle_calls_failed += 1;
        }
    }

    // ---- ExecutionSchedule diagnostic (§9.4) ----
    // Compute the parallelism potential from per-call write key sets.
    // This does not change execution order — it logs what COULD be parallel
    // when wasmer supports concurrent Runtime instances (Tier 6).
    if !per_call_keys.is_empty() {
        let schedule = ExecutionSchedule::build(&per_call_keys);
        let total = schedule.total_calls();
        let max_par = schedule.max_parallelism();
        let score = if total > 0 { max_par as f64 / total as f64 } else { 0.0 };
        info!(
            target: "execution",
            "ExecutionSchedule: {} calls, {} waves, max_parallelism={}, fully_parallel={}, score={:.2}",
            total,
            schedule.wave_count(),
            max_par,
            schedule.is_fully_parallel(),
            score,
        );
    }

    if calls_failed > 0 {
        error!(target: "execution", "Block had {} failed calls out of {}",
            calls_failed, calls_executed + calls_failed);
    } else if calls_executed > 0 {
        info!(target: "execution", "Block executed {} calls successfully", calls_executed);
    }

    // ---- Deployooor post-processing ----
    // HAZID H-M3: Track Deployooor-written keys to detect conflicts with
    // already-merged canonical/uncle state changes. Deployooor WASM execution
    // can write arbitrary contract state keys. If it writes a key already
    // written by a canonical or uncle call, the conflict must be detected.
    for (wasm, contract_id, ix) in &pending_deployments {
        info!(target: "execution", "Deploying {:?} via Deployooor", contract_id);
        // Snapshot existing overlay keys before deployment
        let before_keys: HashSet<Vec<u8>> = {
            let state = main_overlay.state.clone();
            state.cache.keys()
                .chain(state.removed.iter())
                .map(|k| k.to_vec())
                .collect()
        };
        deploy_contract_in_overlay(
            &mut main_overlay, (*store).clone(), vm.clone(),
            wasm, *contract_id, ix, current_height, difficulty,
        )?;
        // Detect new keys introduced by the deployment
        let after_state = main_overlay.state.clone();
        for key in after_state.cache.keys().chain(after_state.removed.iter()) {
            let key_vec = key.to_vec();
            if !before_keys.contains(&key_vec) {
                if !written_keys.insert(key_vec) {
                    return Err(Error::Custom(
                        "DuplicateKeyConflict: Deployooor wrote key already in overlay".to_string()
                    ));
                }
            }
        }
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
    store: &crate::LinearStore,
    backend: Arc<TxBackend>,
    target_cid_bytes: &[u8],
    payload: &[u8],
    current_height: BlockHeight,
    difficulty: BlockTarget,
    tx_hash_bytes: dwow_sdk::tx::TransactionHash,
) -> bool {
    let target_wasm_bytes = match store.get_contract_data(target_cid_bytes) {
        Ok(b) if !b.is_empty() => b,
        _ => { error!(target: "execution", "spend_hook: target contract not found"); return false; }
    };

    let cid_arr: [u8; 32] = match target_cid_bytes[0..32].try_into() {
        Ok(arr) => arr,
        Err(_) => { error!(target: "execution", "spend_hook: CID bytes too short"); return false; }
    };
    let target_cid = match ContractId::from_bytes(cid_arr) {
        Ok(c) => c,
        Err(_) => { error!(target: "execution", "spend_hook: invalid target CID"); return false; }
    };

    let mut target_runtime = match dwow_core::runtime::vm_runtime::Runtime::new(
        &target_wasm_bytes, backend.clone(), target_cid,
        current_height, difficulty, tx_hash_bytes, 0u8,
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
    store: crate::LinearStore,
    vm: Arc<RandomXVM>,
    wasm: &[u8],
    contract_id: ContractId,
    ix: &[u8],
    current_height: BlockHeight,
    difficulty: BlockTarget,
) -> dwow_core::Result<()> {
    overlay.state.cache.insert(
        sled_overlay::sled::IVec::from(contract_id.to_bytes().as_slice()),
        sled_overlay::sled::IVec::from(wasm),
    );

    let deploy_overlay = Arc::new(std::sync::Mutex::new(overlay.clone()));
    let backend = Arc::new(TxBackend {
        overlay: Arc::clone(&deploy_overlay),
        store: Arc::new(store),
        height: current_height,
        vm,
    });
    let mut runtime = dwow_core::runtime::vm_runtime::Runtime::new(
        wasm, backend.clone(), contract_id, current_height,
        difficulty, dwow_sdk::tx::TransactionHash::none(), 0,
    ).map_err(|e| Error::Custom(format!("DeployV1 runtime: {}", e)))?;
    runtime.deploy(ix).map_err(|e| Error::Custom(format!("DeployV1 init: {}", e)))?;
    drop(runtime);

    // Clone the overlay out of the shared Arc — the deploy runtime may hold
    // an extra reference, so try_unwrap is unreliable here.
    *overlay = deploy_overlay.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Ok(())
}

/// The 9 genesis contracts, in canonical bootstrap order (genesis.md
/// Bootstrap Sequence: consensus-critical first, then ecosystem).
///
/// The genesis block carries these deployments as transactions at positions
/// 1..=9 (position 0 is the PoWRewardV1 coinbase). ORDER IS CONSENSUS:
/// position i of the genesis block MUST deploy to `genesis_contracts()[i].0`
/// and declare `singleton_name == genesis_contracts()[i].1`.
pub fn genesis_contracts() -> [(ContractId, &'static str); 9] {
    [
        (*DEPLOYOOOR_CONTRACT_ID, "Deployooor"),
        (*NATIVE_TOKEN_CONTRACT_ID, "NativeToken"),
        (*PROMISSORY_NOTE_CONTRACT_ID, "PromissoryNote"),
        (*IDENTITY_CONTRACT_ID, "Identity"),
        (*ORACLE_CONTRACT_ID, "Oracle"),
        (*ATTESTATION_CONTRACT_ID, "Attestation"),
        (*PURSE_CONTRACT_ID, "Purse"),
        (*BOX_CONTRACT_ID, "Box"),
        (*MULTISIG_CONTRACT_ID, "MultiSig"),
    ]
}

/// Recognizer for a genesis contract-deployment transaction.
///
/// A genesis deployment tx carries exactly one call targeting Deployooor with
/// the DeployV1 selector (`data[0] == 0x00` followed by `DeployParamsV1`),
/// and no inputs, outputs, or nullifiers. These txs are consumed by
/// [`apply_genesis_deployments`] at height GENESIS — they are NOT dispatched
/// through Deployooor WASM (Deployooor itself is one of the payloads).
pub fn is_genesis_deployment_tx(tx: &crate::Transaction) -> bool {
    tx.contract_calls.len() == 1
        && tx.inputs.is_empty()
        && tx.outputs.is_empty()
        && tx.nullifiers.is_empty()
        && tx.contract_calls[0].contract_id == *DEPLOYOOOR_CONTRACT_ID
        && tx.contract_calls[0].data.len() > 1
        && tx.contract_calls[0].data[0] == 0x00
}

/// Genesis-deployment consensus rule (hard-coded genesis).
///
/// At height GENESIS the block carries the 9 contract deployments as
/// transactions — P2P sync provides EVERYTHING a syncing node needs. This
/// rule materializes them into the block's overlay BEFORE any call executes,
/// identically whether the block was built locally (`create_genesis=true`)
/// or received via sync. A node either creates genesis or syncs it — never
/// both. Authenticity of the deployment bytes is the pinned genesis hash:
/// the WASM rides in transactions, so the merkle root (verified by
/// `check_block_header` on every acceptance path) commits to it.
///
/// Validation — ALL failures reject the block:
/// 1. Exactly 1 coinbase + 9 deployment txs, nothing else.
/// 2. Each deployment tx at position i decodes `DeployParamsV1` with
///    `singleton == true` and `singleton_name == genesis_contracts()[i].1`
///    (position binding — order is consensus) and non-empty WASM.
/// 3. Deployment lands at the WELL-KNOWN ContractId from the hard-coded
///    table, NOT `derive_public` — native contracts live at fixed IDs.
/// 4. `__initialize` runs via `deploy_contract_in_overlay` with `ix = &[]`,
///    byte-identical to the historical startup deployment semantics.
///    Manifest bytes (params.ix) are a host-side overlay write.
pub fn apply_genesis_deployments(
    overlay: &mut SledTreeOverlay,
    store: crate::LinearStore,
    vm: Arc<RandomXVM>,
    block: &crate::Block,
    difficulty: BlockTarget,
) -> dwow_core::Result<()> {
    let table = genesis_contracts();

    // 1 coinbase + 9 deployments — exactly, nothing else in genesis.
    if block.transactions.len() != 1 + table.len() {
        return Err(Error::Custom(format!(
            "genesis block has {} txs — must be exactly {} (1 coinbase + {} deployments)",
            block.transactions.len(), 1 + table.len(), table.len(),
        )));
    }

    for (i, (contract_id, name)) in table.iter().enumerate() {
        let tx = &block.transactions[i + 1];
        if !is_genesis_deployment_tx(tx) {
            return Err(Error::Custom(format!(
                "genesis tx {} is not a deployment tx (expected {})",
                i + 1, name,
            )));
        }
        let call = &tx.contract_calls[0];
        let params = DeployParamsV1::decode(&mut Cursor::new(&call.data[1..]))
            .map_err(|e| Error::Custom(format!(
                "genesis deployment {} ({}): DeployParamsV1 decode: {}", i + 1, name, e,
            )))?;
        if !params.singleton || params.singleton_name != *name {
            return Err(Error::Custom(format!(
                "genesis deployment {} order violation: declared '{}', expected '{}'",
                i + 1, params.singleton_name, name,
            )));
        }
        if params.wasm_bincode.is_empty() {
            return Err(Error::Custom(format!(
                "genesis deployment {} ({}): empty WASM", i + 1, name,
            )));
        }

        // Deploy to the WELL-KNOWN id (NOT derive_public) and run
        // __initialize with ix = &[] — same semantics as the historical
        // startup deployment (runtime.deploy(&[])).
        deploy_contract_in_overlay(
            overlay, store.clone(), vm.clone(),
            &params.wasm_bincode, *contract_id, &[],
            BlockHeight::GENESIS, difficulty,
        )?;

        // Manifest bytes ride in params.ix (host-side metadata write, keyed
        // contract_id || b"_manifest" — same key as store.set_contract_data
        // used before). Empty for Deployooor and NativeToken.
        if !params.ix.is_empty() {
            let mut manifest_key = Vec::from(contract_id.to_bytes().as_slice());
            manifest_key.extend_from_slice(b"_manifest");
            overlay.state.cache.insert(
                sled_overlay::sled::IVec::from(manifest_key.as_slice()),
                sled_overlay::sled::IVec::from(params.ix.as_slice()),
            );
        }
    }

    // Pipeline positive discriminator: contracts materialized via genesis
    // block execution (creation on the authority, sync everywhere else).
    info!(target: "execution",
        "Genesis deployments applied: 9 contracts at well-known ContractIds");
    Ok(())
}

// ---------------------------------------------------------------------------
// TxBackend — WASM runtime backend for contract execution
// Moved here from the deleted `blockchain.rs` god object.
// ---------------------------------------------------------------------------

impl TxBackend {
    fn composite_key(tree: &[u8], key: &[u8]) -> Vec<u8> {
        let mut ck = Vec::with_capacity(tree.len() + key.len());
        ck.extend_from_slice(tree);
        ck.extend_from_slice(key);
        ck
    }
}

impl RuntimeBackend for TxBackend {
    fn contract_lookup(&self, cid: &ContractId, tree_name: &str) -> dwow_core::Result<[u8; 32]> {
        let handle = cid.hash_state_id(tree_name);
        let handle_str = format!("{:?}", handle);
        let ov = self.overlay.lock().unwrap_or_else(|e| e.into_inner());
        match ov.get(handle_str.as_bytes()) {
            Ok(Some(iv)) if !iv.is_empty() => return Ok(handle),
            Ok(Some(_)) => return Err(Error::ContractStateNotFound),
            Ok(None) => {}
            Err(e) => return Err(Error::Custom(e.to_string())),
        }
        drop(ov);
        let data = self.store.get_contract_data(handle_str.as_bytes())
            .map_err(|e| Error::Custom(e.to_string()))?;
        if data.is_empty() {
            return Err(Error::ContractStateNotFound)
        }
        Ok(handle)
    }

    fn contract_init(&self, cid: &ContractId, tree_name: &str) -> dwow_core::Result<[u8; 32]> {
        let handle = cid.hash_state_id(tree_name);
        let handle_str = format!("{:?}", handle);
        self.overlay.lock().unwrap_or_else(|e| e.into_inner())
            .insert(handle_str.as_bytes(), tree_name.as_bytes())
            .map_err(|e| Error::Custom(e.to_string()))?;
        Ok(handle)
    }

    fn contract_insert_bincode(&self, cid: ContractId, bincode: &[u8]) -> dwow_core::Result<()> {
        self.overlay.lock().unwrap_or_else(|e| e.into_inner())
            .insert(&cid.to_bytes(), bincode)
            .map_err(|e| Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn contract_get_bincode(&self, cid: &ContractId) -> dwow_core::Result<Vec<u8>> {
        let ov = self.overlay.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(Some(iv)) = ov.get(&cid.to_bytes()) {
            if iv.is_empty() {
                return Err(Error::ContractStateNotFound);
            }
            return Ok(iv.to_vec());
        }
        drop(ov);
        let data = self.store.get_contract_data(&cid.to_bytes())
            .map_err(|e| Error::Custom(e.to_string()))?;
        if data.is_empty() {
            return Err(Error::ContractStateNotFound)
        }
        Ok(data)
    }

    fn db_insert(&self, tree: &[u8], key: &[u8], value: &[u8]) -> dwow_core::Result<()> {
        let ck = Self::composite_key(tree, key);
        self.overlay.lock().unwrap_or_else(|e| e.into_inner())
            .insert(&ck, value)
            .map_err(|e| Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn db_get(&self, tree: &[u8], key: &[u8]) -> dwow_core::Result<Option<Vec<u8>>> {
        let ck = Self::composite_key(tree, key);
        let ov = self.overlay.lock().unwrap_or_else(|e| e.into_inner());
        match ov.get(&ck) {
            Ok(Some(iv)) => {
                if iv.is_empty() { return Ok(None); }
                return Ok(Some(iv.to_vec()));
            }
            Ok(None) => {}
            Err(e) => return Err(Error::Custom(e.to_string())),
        }
        drop(ov);
        let data = self.store.get_contract_data(&ck)
            .map_err(|e| Error::Custom(e.to_string()))?;
        if data.is_empty() { Ok(None) } else { Ok(Some(data)) }
    }

    fn db_remove(&self, tree: &[u8], key: &[u8]) -> dwow_core::Result<()> {
        let ck = Self::composite_key(tree, key);
        self.overlay.lock().unwrap_or_else(|e| e.into_inner())
            .insert(&ck, &[])
            .map_err(|e| Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn db_contains_key(&self, tree: &[u8], key: &[u8]) -> dwow_core::Result<bool> {
        let ck = Self::composite_key(tree, key);
        let ov = self.overlay.lock().unwrap_or_else(|e| e.into_inner());
        match ov.get(&ck) {
            Ok(Some(iv)) => return Ok(!iv.is_empty()),
            Ok(None) => {}
            Err(e) => return Err(Error::Custom(e.to_string())),
        }
        drop(ov);
        let data = self.store.get_contract_data(&ck)
            .map_err(|e| Error::Custom(e.to_string()))?;
        Ok(!data.is_empty())
    }

    fn last_block_timestamp(&self) -> dwow_core::Result<Vec<u8>> {
        if self.height.get() == 0 {
            return Ok(0u64.to_le_bytes().to_vec())
        }
        let block = self.store.get_block(self.height).map_err(|e| Error::Custom(e.to_string()))?;
        Ok(block.header.timestamp.to_le_bytes().to_vec())
    }

    fn last_block_height(&self) -> dwow_core::Result<BlockHeight> {
        Ok(self.height)
    }

    /// Returns a transaction as serde_json bytes.
    /// WARNING: JSON serialization is NOT deterministic across serde versions.
    /// Do NOT use the returned bytes for consensus-critical computations.
    /// Currently zero contracts call this function (2026-07-22 MOC audit).
    /// Closes: H5 (WASM get_tx returns non-deterministic serde_json bytes).
    fn get_tx(&self, hash: &[u8; 32]) -> dwow_core::Result<Option<Vec<u8>>> {
        match self.store.get_transaction(hash) {
            Ok(tx) => {
                let data = serde_json::to_vec(&tx).map_err(|e| Error::Custom(e.to_string()))?;
                Ok(Some(data))
            }
            Err(e) => {
                if e.to_string().contains("TransactionNotFound") {
                    Ok(None)
                } else {
                    Err(Error::Custom(e.to_string()))
                }
            }
        }
    }

    fn get_tx_location(&self, hash: &[u8; 32]) -> dwow_core::Result<Option<Vec<u8>>> {
        for h in 1..=self.height.get() {
            if let Ok(block) = self.store.get_block(BlockHeight::new(h)) {
                for (tx_index, tx) in block.transactions.iter().enumerate() {
                    if tx.hash().as_bytes() == hash {
                        // Payload matches the guest decode in sdk wasm/util.rs
                        // `get_tx_location`: (block_height: u64, tx_index: u16).
                        let tx_index = u16::try_from(tx_index)
                            .map_err(|_| Error::Custom("tx index exceeds u16".to_string()))?;
                        let mut payload = h.to_le_bytes().to_vec();
                        payload.extend_from_slice(&tx_index.to_le_bytes());
                        return Ok(Some(payload))
                    }
                }
            }
        }
        Ok(None)
    }

    fn get_block_hash_by_height(&self, height: BlockHeight) -> dwow_core::Result<Option<Vec<u8>>> {
        match self.store.get_block(height) {
            Ok(block) => Ok(Some(block.hash_with_vm(&self.vm).as_bytes().to_vec())),
            Err(e) => {
                if e.to_string().contains("BlockNotFound") {
                    Ok(None)
                } else {
                    Err(Error::Custom(e.to_string()))
                }
            }
        }
    }
}
