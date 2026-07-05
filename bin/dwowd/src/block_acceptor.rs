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
    let outcome = execute_block(chain_state, block, uncles, vm, current_height, target)?;

    // 4. Aggregate WASM execution overlay into a sled batch.
    let contracts_batch = outcome.overlay.state.aggregate().unwrap_or_default();

    // 5. Connect block with contract state — single atomic sled transaction.
    chain_state.connect_block(block, uncles, Some(contracts_batch))
        .map_err(|e| dwow_core::Error::Custom(format!("connect_block failed: {}", e)))
}
