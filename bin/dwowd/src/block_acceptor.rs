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

use dwow_chain::{Block, CChainState, UncleBlock};
use dwow_core::Result;

use dwow_chain::execution::execute_block;
use dwow_chain::proof_of_token_balance;

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
///    and `CUMULATIVE_BLIND` to the contracts sled tree.
///
/// 3. **Overlay aggregation** — converts the WASM execution overlay
///    (`SledTreeOverlay`) into a `sled::Batch` for atomic commit.
///
/// 4. **Connect block** — commits the block, its contract state overlay,
///    consensus updates, coins, and nullifiers in a single atomic sled
///    transaction.
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
    // C5 fix: a block with invalid PoW should be rejected immediately, not
    // after running expensive WASM contracts. Stage 1 only checks hash_u32 <= target
    // (cheap); Stage 2 (target == expected_target) lives in connect_block.
    // Monero merge-mined blocks skip native RandomX check.
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

    // 3. WASM execution — runs pow_reward_v1, persists cumulative supply chain.
    // Pass block.header.height as verifying height so the contract's
    // expected_reward(verifying_block_height) returns the correct value
    // for THIS block, not the previous tip.
    let outcome = execute_block(chain_state, block, uncles, vm, block.header.height, target)?;

    // 4. Aggregate WASM execution overlay into a sled batch.
    let contracts_batch = outcome.overlay.state.aggregate().unwrap_or_default();

    // 5. Connect block with contract state — single atomic sled transaction.
    chain_state.connect_block(block, uncles, Some(contracts_batch))
        .map_err(|e| dwow_core::Error::Custom(format!("connect_block failed: {}", e)))?;

    // 6. Mirror cumulative supply state from contracts tree to supply_chain tree.
    // The WASM contract writes cumulative state to the contracts tree via
    // apply_pow_reward (through the SledTreeOverlay). After connect_block
    // commits the overlay, we read those values and mirror them to the
    // supply_chain tree. This ensures the single-source-of-truth module
    // always has the latest state for host-side coinbase building.
    mirror_cumulative_state(chain_state, block.header.height)?;

    Ok(())
}

/// Read cumulative supply state from the contracts sled tree and mirror it
/// to the supply_chain module. Called after connect_block commits the
/// WASM execution overlay.
///
/// The contracts tree is written by the WASM contract's apply_pow_reward
/// via the SledTreeOverlay. The supply_chain tree is the host-side cache
/// used by the coinbase builder. Both must stay in sync.
fn mirror_cumulative_state(
    chain_state: &dwow_chain::CChainState,
    height: u64,
) -> dwow_core::Result<()> {
    use dwow_native_token_contract::{
        NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND,
        NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT,
        NATIVE_TOKEN_CONTRACT_INFO_TREE,
        NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY,
    };
    use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;
    use dwow_sdk::crypto::pasta_prelude::Group;

    let contracts = chain_state.store.contracts_tree();
    let info_prefix = NATIVE_TOKEN_CONTRACT_ID.hash_state_id(NATIVE_TOKEN_CONTRACT_INFO_TREE);

    // Read TOTAL_SUPPLY from contracts tree
    let mut total_supply_key = Vec::from(info_prefix.as_slice());
    total_supply_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY);
    let total_supply: u64 = contracts
        .get(sled::IVec::from(total_supply_key.as_slice()))
        .ok()
        .flatten()
        .map(|v| dwow_serial::deserialize(&v).unwrap_or(0))
        .unwrap_or(0);

    // Read CUMULATIVE_VALUE_COMMIT from contracts tree
    let mut cum_key = Vec::from(info_prefix.as_slice());
    cum_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT);
    let value_commit = contracts
        .get(sled::IVec::from(cum_key.as_slice()))
        .ok()
        .flatten()
        .map(|v| dwow_serial::deserialize::<dwow_sdk::pasta::pallas::Point>(&v).unwrap_or_default())
        .unwrap_or(dwow_sdk::pasta::pallas::Point::identity());

    // Read CUMULATIVE_BLIND from contracts tree
    let mut blind_key = Vec::from(info_prefix.as_slice());
    blind_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND);
    let blind = contracts
        .get(sled::IVec::from(blind_key.as_slice()))
        .ok()
        .flatten()
        .map(|v| dwow_serial::deserialize::<dwow_sdk::pasta::pallas::Scalar>(&v).unwrap_or_default())
        .unwrap_or(dwow_sdk::pasta::pallas::Scalar::zero());

    use dwow_chain::CumulativeSupplyEntry;
    let entry = CumulativeSupplyEntry {
        value_commit,
        blind,
        total_supply,
    };
    chain_state.supply_chain.commit(height, &entry)
        .map_err(|e| dwow_core::Error::Custom(format!("supply_chain mirror: {}", e)))?;

    Ok(())
}
