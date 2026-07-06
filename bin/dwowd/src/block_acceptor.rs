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
) -> Result<()> {
    // 1. Proof of token balance — no hidden darkw minting beyond the coinbase.
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

    // 3. WASM execution — runs pow_reward_v1, persists cumulative supply chain
    // to the contracts sled tree via the overlay.
    let outcome = execute_block(chain_state, block, uncles, vm, block.header.height, target)?;

    // 4. Read cumulative supply state from the WASM execution overlay BEFORE
    // aggregation. This is the single bridge point between the contracts tree
    // (where WASM writes) and the supply_chain tree (where the host reads).
    // Reading from the overlay ensures we capture what the WASM just wrote,
    // without relying on a post-commit mirror.
    let (supply_chain_batch, sc_entry) = read_cumulative_from_overlay(
        chain_state, &outcome.overlay, block.header.height)?;

    // 5. Aggregate WASM execution overlay into a sled batch.
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

    // Read TOTAL_SUPPLY from overlay
    let mut total_supply_key = Vec::from(info_prefix.as_slice());
    total_supply_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY);
    let total_supply: u64 = overlay
        .get(&total_supply_key)
        .ok()
        .flatten()
        .map(|v| dwow_serial::deserialize(&v).unwrap_or(0))
        .unwrap_or(0);

    // Read CUMULATIVE_VALUE_COMMIT from overlay
    let mut cum_key = Vec::from(info_prefix.as_slice());
    cum_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT);
    let value_commit: dwow_sdk::pasta::pallas::Point = overlay
        .get(&cum_key)
        .ok()
        .flatten()
        .map(|v| dwow_serial::deserialize(&v).unwrap_or_else(|_| dwow_sdk::pasta::pallas::Point::identity()))
        .unwrap_or_else(dwow_sdk::pasta::pallas::Point::identity);

    // Read CUMULATIVE_BLIND from overlay
    let mut blind_key = Vec::from(info_prefix.as_slice());
    blind_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND);
    let blind: dwow_sdk::pasta::pallas::Scalar = overlay
        .get(&blind_key)
        .ok()
        .flatten()
        .map(|v| dwow_serial::deserialize(&v).unwrap_or_else(|_| dwow_sdk::pasta::pallas::Scalar::zero()))
        .unwrap_or_else(dwow_sdk::pasta::pallas::Scalar::zero);

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
