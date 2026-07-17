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

//! Single unified block acceptance path.
//!
//! Every block enters the chain through exactly one function. The five entry
//! points (built-in miner, stratum, mm_rpc, miner_rpc, P2P broadcast) differ
//! only in how they *obtain* the block and VM — not in how they *accept* it.
//!
//! Design principle: one module, one responsibility. This module owns the
//! accept-or-reject decision. No other module duplicates this logic.

use std::sync::Arc;

use dwow_chain::{Block, CChainState, CumulativeSupplyEntry, UncleBlock};
use dwow_core::Result;

use dwow_chain::execution::execute_block;
use dwow_chain::proof_of_token_balance;
use sled_overlay::SledTreeOverlay;

/// Accept a fully-validated block into the chain.
///
/// This is the **single** block acceptance path. All five entry points
/// (built-in miner, stratum, mm_rpc, miner_rpc, P2P broadcast) call this
/// function after they have obtained a block and a RandomX VM.
///
/// # Pipeline
///
/// 1. **Proof of token balance** — verifies per-block mass balance:
///    `Σ outputs + burns + fees == Σ inputs`. Rejects blocks with
///    hidden inflation beyond the coinbase.
///
/// 2. **WASM execution** — runs contract calls including `pow_reward_v1`
///    for the coinbase. Persists `TOTAL_SUPPLY`, `CUMULATIVE_VALUE_COMMIT`,
///    and `CUMULATIVE_BLIND` to the contracts sled tree via the overlay.
///
/// 3. **Read cumulative state from overlay** — before aggregation, reads
///    the cumulative supply values from the WASM execution overlay and
///    builds a `CumulativeSupplyEntry` for the supply_chain tree.
///
/// 4. **Overlay aggregation** — converts the WASM execution overlay
///    (`SledTreeOverlay`) into a `sled::Batch` for atomic commit.
///
/// 5. **Atomic commit** — commits the block, its contract state overlay,
///    the supply_chain entry, consensus updates, coins, and nullifiers
///    in a single atomic sled transaction. No post-commit mirror needed.
///
/// # Errors
///
/// Returns `Err` if any step fails. The caller should not retry with the
/// same block — the block is invalid and should be discarded.
pub fn accept_block(
    chain_state: &CChainState,
    block: &Block,
    uncles: &[UncleBlock],
    vm: &Arc<randomx::RandomXVM>,
    current_height: u64,
    target: u32,
    fee_estimator: Option<&std::sync::Arc<dwow_chain::fee_estimator::FeeEstimator>>,
) -> Result<()> {
    // 1. Proof of token balance — no hidden darkw minting beyond the coinbase.
    // 0. Phase 0 structural validation — cheapest check first.
    // Per formal guardrail: VALID_COINBASE rejects blocks with missing,
    // misplaced, or null coinbase before any expensive validation.
    dwow_chain::validation::validate_block_structure(block)
        .map_err(|e| dwow_core::Error::Custom(format!("Block structure invalid: {}", e)))?;

    proof_of_token_balance::verify_proof_of_token_balance(block)
        .map_err(|e| dwow_core::Error::Custom(format!("Proof of token balance failed: {}", e)))?;

    // 2. Stage 1 PoW — verify hash meets target BEFORE expensive WASM execution.
    if !matches!(block.header.pow_source, dwow_chain::PowSource::Monero(_)) {
        let block_hash = block.hash_with_vm(vm.as_ref());
        let hash_u32 = u32::from_le_bytes(block_hash.as_bytes()[0..4].try_into().unwrap());
        if hash_u32 > target {
            return Err(dwow_core::Error::Custom(format!(
                "Block PoW invalid: hash={:#010x} target={:#010x}",
                hash_u32, target
            )));
        }
    }

    // 2.5 L2 transaction verification — for every non-coinbase tx, decode the
    // witness (L1 carried it; L1 barrier #1 keeps it out of the block hash),
    // reconcile it against the chain tx, and verify ZK proofs + signatures.
    // Per mempool.md §1/§4, verified at both admission and block accept.
    // Failure here means a fabricated/unauthenticated tx — reject the block.
    // Coinbase is exempt (soundness = transparent WASM re-execution).
    // Full metadata-based VK verification is done inside execute_block.
    for tx in &block.transactions {
        let is_coinbase = tx.contract_calls.first()
            .map_or(false, |c| c.data.first() == Some(&0x05));
        if is_coinbase {
            continue;
        }
        if let Err(e) = dwow_chain::zk_verifier::verify_single_tx(tx) {
            match e {
                // Per mempool.md §1: the witness is load-bearing. A non-coinbase
                // transaction without a witness cannot be authenticated. Coinbase
                // is exempt (soundness = transparent WASM re-execution).
                dwow_chain::zk_verifier::VerifyError::NoWitness => {
                    return Err(dwow_core::Error::Custom(
                        "Non-coinbase transaction missing witness — per mempool.md §1, witness is load-bearing".into()
                    ));
                }
                _ => return Err(dwow_core::Error::Custom(format!(
                    "L2 witness verification failed: {}", e
                ))),
            }
        }
    }

    // 2.6 Host-side reward check (Phase 3.4 compliance fix):
    // Verify coinbase reward meets emission schedule BEFORE expensive WASM execution.
    // Previously deferred entirely to WASM pow_reward_v1. Now enforced at host level
    // for fail-fast — reject under-reward blocks before spawning WASM runtime.
    {
        let expected = dwow_sdk::blockchain::expected_reward(block.header.height);
        if block.header.total_reward < expected {
            return Err(dwow_core::Error::Custom(format!(
                "Coinbase reward {} below expected_reward({}) = {}",
                block.header.total_reward, block.header.height, expected
            )));
        }
    }

    // 3. WASM execution — runs pow_reward_v1, persists cumulative supply chain
    // to the contracts sled tree via the overlay.
    let outcome = match execute_block(chain_state, block, uncles, vm, block.header.height, target) {
        Ok(o) => o,
        Err(e) => {
            // Supply mismatches during bootstrap (heights 1-2) are expected
            // while TOTAL_SUPPLY is being seeded. Log with full context so
            // operators can distinguish transient sync issues from real bugs.
            let local_height = chain_state.get_height();
            tracing::warn!(
                target: "block_acceptor",
                "Block {} rejected at local height {}: {}",
                block.header.height, local_height, e
            );
            return Err(e);
        }
    };

    // Fee estimator sampling — every accepted block contributes its actual
    // gas usage to the rolling window (MOC close-out item 10.3). Previously
    // only the self-mining path called record_block; non-mining nodes never
    // sampled, reporting stale/MIN_FEE estimates permanently.
    // smol::block_on: accept_block is a sync fn in a sync call chain, but
    // record_block is async. Running the estimator update synchronously in
    // this path adds negligible latency (<1ms atomic pushes) to block accept.
    if let Some(estimator) = fee_estimator {
        smol::block_on(estimator.record_block(outcome.stats.gas_used));
    }

    // 4. Read cumulative supply state from the WASM execution overlay BEFORE
    // aggregation. This is the single bridge point between the contracts tree
    // (where WASM writes) and the supply_chain tree (where the host reads).
    // Reading from the overlay ensures we capture what the WASM just wrote,
    // without relying on a post-commit mirror.
    let (supply_chain_batch, sc_entry) = read_cumulative_from_overlay(
        chain_state, &outcome.overlay, block.header.height)?;

    // Defense-in-depth: every block MUST write cumulative supply state.
    // If WASM execution failed silently (empty overlay, deserialization
    // default), the chain would be bricked — subsequent blocks cannot
    // validate S_{H-1}. Fail hard so the operator can investigate.
    if supply_chain_batch.is_none() {
        return Err(dwow_core::Error::Custom(format!(
            "Block at height {} MUST write cumulative supply state; \
             WASM execution may have failed silently (empty overlay)",
            block.header.height
        )));
    }

    // 5. Aggregate WASM execution overlay into a sled batch.
    // Empty overlay is valid for blocks with no contract calls beyond coinbase.
    // The supply_chain_batch.is_none() guard above catches the critical case
    // where cumulative supply state was expected but not written.
    let contracts_batch = outcome.overlay.state.aggregate().unwrap_or_default();

    // 6. Atomic commit — blocks, contracts, supply_chain, consensus, coins,
    // and nullifiers all committed in a single sled transaction.
    chain_state.connect_block(block, uncles, Some(contracts_batch), supply_chain_batch)
        .map_err(|e| dwow_core::Error::Custom(format!("connect_block failed: {}", e)))?;

    // 7. Update in-memory cache AFTER the atomic transaction succeeds.
    // The sled write was atomic; now the cache must reflect the new state.
    if let Some(entry) = sc_entry {
        chain_state.supply_chain.update_cache(block.header.height, entry);
    }

    Ok(())
}

/// Read cumulative supply state from the WASM execution overlay and build
/// a `CumulativeSupplyEntry` for the supply_chain tree.
///
/// The WASM `pow_reward_v1` writes cumulative state to the contracts tree
/// via the `SledTreeOverlay`. This function reads those values BEFORE
/// overlay aggregation, then uses `commit_to_batch()` to prepare an atomic
/// write to the supply_chain tree.
///
/// Returns:
/// - `Some(sled::Batch)` — the supply_chain batch to include in the atomic
///   transaction (None if no cumulative values were found — genesis or error)
/// - `Option<CumulativeSupplyEntry>` — the entry for post-commit cache update
fn read_cumulative_from_overlay(
    chain_state: &dwow_chain::CChainState,
    overlay: &sled_overlay::SledTreeOverlay,
    height: u64,
) -> dwow_core::Result<(Option<sled::Batch>, Option<dwow_chain::CumulativeSupplyEntry>)> {
    use dwow_native_token_contract::{
        NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND,
        NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT,
        NATIVE_TOKEN_CONTRACT_INFO_TREE,
        NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY,
    };
    use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;
    use dwow_sdk::crypto::pasta_prelude::Group;

    let info_prefix = NATIVE_TOKEN_CONTRACT_ID.hash_state_id(NATIVE_TOKEN_CONTRACT_INFO_TREE);

    // Read TOTAL_SUPPLY from overlay — fail closed, never silently default to zero.
    // A missing or corrupt TOTAL_SUPPLY means WASM execution failed; substituting
    // zero would brick the cumulative supply chain for all subsequent blocks.
    let mut total_supply_key = Vec::from(info_prefix.as_slice());
    total_supply_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY);
    let total_supply_raw = overlay
        .get(&total_supply_key)
        .map_err(|e| dwow_core::Error::Custom(format!("overlay read total_supply: {}", e)))?
        .ok_or_else(|| dwow_core::Error::Custom(
            "total_supply not found in WASM execution overlay".into()
        ))?;
    let total_supply: u64 = dwow_serial::deserialize(&total_supply_raw)
        .map_err(|e| dwow_core::Error::Custom(format!("deserialize total_supply: {}", e)))?;

    // Read CUMULATIVE_VALUE_COMMIT from overlay — fail closed.
    let mut cum_key = Vec::from(info_prefix.as_slice());
    cum_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT);
    let value_commit_raw = overlay
        .get(&cum_key)
        .map_err(|e| dwow_core::Error::Custom(format!("overlay read value_commit: {}", e)))?
        .ok_or_else(|| dwow_core::Error::Custom(
            "cumulative_value_commit not found in WASM execution overlay".into()
        ))?;
    let value_commit: dwow_sdk::pasta::pallas::Point = dwow_serial::deserialize(&value_commit_raw)
        .map_err(|e| dwow_core::Error::Custom(format!("deserialize value_commit: {}", e)))?;

    // Read CUMULATIVE_BLIND from overlay — fail closed.
    let mut blind_key = Vec::from(info_prefix.as_slice());
    blind_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND);
    let blind_raw = overlay
        .get(&blind_key)
        .map_err(|e| dwow_core::Error::Custom(format!("overlay read blind: {}", e)))?
        .ok_or_else(|| dwow_core::Error::Custom(
            "cumulative_blind not found in WASM execution overlay".into()
        ))?;
    let blind: dwow_sdk::pasta::pallas::Scalar = dwow_serial::deserialize(&blind_raw)
        .map_err(|e| dwow_core::Error::Custom(format!("deserialize blind: {}", e)))?;

    // If no cumulative state was written (e.g., genesis block with no WASM),
    // return None — nothing to mirror.
    if total_supply == 0 && value_commit == dwow_sdk::pasta::pallas::Point::identity() {
        return Ok((None, None));
    }

    use dwow_chain::CumulativeSupplyEntry;
    let entry = CumulativeSupplyEntry {
        value_commit,
        blind,
        total_supply,
    };

    // Build the supply_chain batch for atomic commit
    let mut batch = sled::Batch::default();
    chain_state.supply_chain.commit_to_batch(&mut batch, height, &entry)
        .map_err(|e| dwow_core::Error::Custom(format!("supply_chain commit_to_batch: {}", e)))?;

    Ok((Some(batch), Some(entry)))
}
