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

use crate::execution::execute_block;
use crate::proof_of_token_balance;

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

    // 2. WASM execution — runs pow_reward_v1, persists cumulative supply chain.
    let outcome = execute_block(chain_state, block, uncles, vm, current_height, target)?;

    // 3. Aggregate WASM execution overlay into a sled batch.
    let contracts_batch = outcome.overlay.state.aggregate().unwrap_or_default();

    // 4. Connect block with contract state — single atomic sled transaction.
    chain_state.connect_block(block, uncles, Some(contracts_batch))
        .map_err(|e| dwow_core::Error::Custom(format!("connect_block failed: {}", e)))
}
