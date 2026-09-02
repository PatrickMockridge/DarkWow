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
//! Spec: sync-protocol.md §1 (BlockSink = `accept_block`), §19 (reorg & disconnect:
//! `reorganize_to_chain`, `perform_reorg`, `rollback_cumulative_commit`, contracts-tree
//! `CBlockUndo`); consensus.md §Fork Choice Rule (heaviest-chain).
//!
//! Every block enters the chain through exactly one function. The five entry
//! points (built-in miner, stratum, mm_rpc, miner_rpc, P2P broadcast) differ
//! only in how they *obtain* the block and VM — not in how they *accept* it.
//!
//! Design principle: one module, one responsibility. This module owns the
//! accept-or-reject decision. No other module duplicates this logic.

use std::sync::Arc;

use dwow_chain::{Block, BlockConnectOutcome, CChainState, UncleBlock};
use dwow_core::Result;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, SupplyAmount};

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
///    the supply_chain entry, consensus updates, commitment_set, and nullifiers
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
    _current_height: BlockHeight,
    target: BlockTarget,
    fee_estimator: Option<&std::sync::Arc<dwow_chain::fee_estimator::FeeEstimator>>,
) -> Result<BlockConnectOutcome> {
    // 1. Proof of token balance — no hidden darkw minting beyond the coinbase.
    // 0. Phase 0 structural validation — cheapest check first.
    // Per formal guardrail: VALID_COINBASE rejects blocks with missing,
    // misplaced, or null coinbase before any expensive validation.
    dwow_chain::validation::validate_block_structure(block)
        .map_err(|e| dwow_core::Error::Custom(format!("Block {} structure invalid: {}", block.header.height, e)))?;
    tracing::debug!(target: "block_acceptor", "structure validated");

    // 0.2 Uncle validation — HAZOP F5: check_uncles() was dead code (zero
    // non-test callers). Wire it into the acceptance path after structural
    // validation, before expensive WASM execution. 6 checks enforced:
    // max count, merkle root, PoW, proofs, depth, dedup.
    if !uncles.is_empty() {
        let (expected_root, proofs) = dwow_chain::build_uncle_merkle(uncles, vm)
            .map_err(|e| dwow_core::Error::Custom(format!("uncle merkle: {e}")))?;
        if expected_root != block.header.uncle_merkle_root {
            return Err(dwow_core::Error::Custom(format!(
                "Block {} uncle merkle root mismatch: computed {:?}, header has {:?}",
                block.header.height, expected_root, block.header.uncle_merkle_root
            )));
        }
        // HAZOP H25 fix: collect existing uncle hashes from sled for cross-block dedup.
        // Previously always empty — uncles could earn rewards across multiple blocks.
        let existing_keys: std::collections::HashSet<[u8; 32]> = chain_state.stored_uncle_hashes();
        dwow_chain::validation::check_uncles(
            uncles,
            &proofs,
            &block.header.uncle_merkle_root,
            block.header.height,
            vm,
            block.header.target,
            &existing_keys,
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "Block {} uncle validation failed: {}", block.header.height, e
        )))?;
    }

    // 0.5 Block size — consensus-level acceptance rule (fail-closed), not
    // just a wire cap. The genesis block is EXEMPT: it carries the 9 contract
    // deployments (WASM in transactions), and its integrity check is the
    // pinned genesis hash, not a size cap.
    if block.header.height != BlockHeight::GENESIS {
        let block_len = serde_json::to_vec(block)
            .map_err(|e| dwow_core::Error::Custom(format!("Block serialization: {}", e)))?
            .len();
        // Closes: M5 (block size check uses non-canonical serde_json —
        // different serde versions can produce different byte lengths).
        // A 1% safety margin prevents a block whose JSON size straddles
        // MAX_BLOCK_SIZE across serde versions from causing a chain split.
        // MAX_BLOCK_SIZE is a DOS gate, not a consensus rule — a generous
        // margin is acceptable per type-system.md §8.6.2.
        let max = dwow_chain::execution::MAX_BLOCK_SIZE;
        // Reject if within 1% of max (floor: at least 1 byte margin)
        let soft_limit = max.saturating_sub(max / 100).max(1);
        if block_len > soft_limit {
            return Err(dwow_core::Error::Custom(format!(
                "Block at height {} is {} bytes — within 1% of MAX_BLOCK_SIZE {} (soft limit {})",
                block.header.height, block_len, max, soft_limit
            )));
        }
    }

    proof_of_token_balance::verify_proof_of_token_balance(block)
        .map_err(|e| dwow_core::Error::Custom(format!("Block {} proof of token balance failed: {}", block.header.height, e)))?;
    tracing::debug!(target: "block_acceptor", "token balance verified");

    // 2. Stage 1 PoW — verify hash meets target BEFORE expensive WASM execution.
    // Monero merge-mined blocks skip native RandomX (the PoW comes from the
    // Monero chain) but MUST carry a valid coinbase Merkle proof.
    match &block.header.pow_source {
        dwow_chain::PowSource::Monero(monero_data) => {
            // HAZOP C6 fix: verify the Monero-side coinbase Merkle proof.
            if !monero_data.is_coinbase_valid_merkle_root() {
                return Err(dwow_core::Error::Custom(format!(
                    "Block {} Monero coinbase Merkle proof invalid",
                    block.header.height
                )));
            }
        }
        _ => {
            let block_hash = block.hash_with_vm(vm.as_ref())
                .map_err(|e| dwow_core::Error::Custom(format!("hash: {e}")))?;
            #[expect(clippy::unwrap_used, reason = "4-byte slice always converts to [u8; 4]")]
            let hash_u32 = u32::from_le_bytes(block_hash.as_bytes()[0..4].try_into().unwrap());
            if !target.hash_is_valid(hash_u32) {
                return Err(dwow_core::Error::Custom(format!(
                    "Block {} PoW invalid: hash={:#010x} target={:#010x}",
                    block.header.height, hash_u32, target
                )));
            }
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
            .map_or(false, |c| c.data.first() == Some(&0x05)
                && c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID);
        if is_coinbase {
            continue;
        }
        // Genesis deployment txs are exempt — same soundness argument as the
        // coinbase exemption above: their authenticity is the pinned genesis
        // hash + merkle root, and their effect is transparent WASM
        // re-execution (__initialize) by apply_genesis_deployments.
        if block.header.height == BlockHeight::GENESIS
            && dwow_chain::execution::is_genesis_deployment_tx(tx)
        {
            continue;
        }
        if let Err(e) = dwow_chain::zk_verifier::verify_single_tx(tx) {
            match e {
                // Per mempool.md §1: the witness is load-bearing. A non-coinbase
                // transaction without a witness cannot be authenticated. Coinbase
                // is exempt (soundness = transparent WASM re-execution).
                dwow_chain::zk_verifier::VerifyError::NoWitness => {
                    return Err(dwow_core::Error::Custom(format!(
                        "Block {}: non-coinbase transaction missing witness — per mempool.md §1, witness is load-bearing",
                        block.header.height
                    )));
                }
                _ => return Err(dwow_core::Error::Custom(format!(
                    "Block {} L2 witness verification failed: {}", block.header.height, e
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
        // Spec: uncle_merkle.md §"Coinbase Split" — subtractive mass balance:
        // total_reward + Σ pin == expected_reward. Enforced here (fail-fast before
        // WASM execution) and again in connect_block via verify_uncle_split.
        // This single exact check subsumes the old lower/upper bound checks:
        // under-reward (total < effective) and over-reward (total > effective) are
        // both violations of the mass balance.
        let total_pin: u64 = uncles
            .iter()
            .filter(|u| u.pin_accepted && u.pin_confirmed > BlockReward::new(0))
            .map(|u| u.pin_confirmed.get())
            .sum();
        let effective = expected.get().saturating_sub(total_pin);
        if block.header.total_reward.get() != effective {
            return Err(dwow_core::Error::Custom(format!(
                "Coinbase reward {} != expected_reward({}) - Σ pin({}) = {}",
                block.header.total_reward, block.header.height, total_pin, effective
            )));
        }

        // H3 — spendable-note mass balance (uncle_merkle.md §"Spendable-note mass
        // balance"). The value-level `total_reward + Σ pin == base` check above is
        // not enough: `effective_value` is otherwise a hidden circuit witness, so a
        // miner could mint the coinbase note at FULL base while emitting no uncle
        // notes (reward theft), or over-emit uncle notes (over-mint by Σ pin). Bind
        // the actually-spendable notes to the consensus reward.
        let pow_selector = dwow_native_token_contract::NativeTokenFunction::PoWRewardV1 as u8;
        let uncle_selector = dwow_native_token_contract::NativeTokenFunction::UncleMintV1 as u8;

        let coinbase_effective = block.transactions.first()
            .and_then(|tx| tx.contract_calls.first())
            .filter(|c| c.data.first() == Some(&pow_selector))
            .and_then(|c| dwow_native_token_contract::model::PoWRewardParamsV1::decode(&c.data[1..]).ok())
            .map(|p| p.effective_value)
            .ok_or_else(|| dwow_core::Error::Custom(
                "coinbase PoWRewardV1 params missing or malformed".to_string()
            ))?;

        if coinbase_effective != block.header.total_reward.get() {
            return Err(dwow_core::Error::Custom(format!(
                "Coinbase spendable note {} != header.total_reward {} (reward theft)",
                coinbase_effective, block.header.total_reward
            )));
        }

        let mut sum_uncle_effective: u64 = 0;
        for tx in &block.transactions {
            for call in &tx.contract_calls {
                if call.data.first() != Some(&uncle_selector) {
                    continue;
                }
                if let Ok(um) = dwow_native_token_contract::model::UncleMintParamsV1::decode(&call.data[1..]) {
                    sum_uncle_effective = sum_uncle_effective.saturating_add(um.effective_value);
                }
            }
        }
        if sum_uncle_effective != total_pin {
            return Err(dwow_core::Error::Custom(format!(
                "Uncle note values sum {} != Σ pin {} (over/under-mint)",
                sum_uncle_effective, total_pin
            )));
        }
    }

    // 2.7 Fork detection BEFORE WASM execution: a block that extends a heavier
    // competing chain must reorg (disconnect + re-accept), not execute WASM
    // against the wrong cumulative-commit state (which fails pow_reward_v1's
    // old_cumulative_commit check). This is the fix for the divergent-coinbase
    // fork stall (node-startup-spec.md §4 known gap).
    match chain_state
        .detect_reorg(block)
        .map_err(|e| dwow_core::Error::Custom(format!("detect_reorg failed: {}", e)))?
    {
        dwow_chain::ReorgSignal::Heavier { fork_height, competing_block } => {
            tracing::info!(target: "block_acceptor", "Reorg (pre-WASM): fork at height {}", fork_height);
            return perform_reorg(chain_state, block, uncles, vm, target, fee_estimator, fork_height, &competing_block);
        }
        dwow_chain::ReorgSignal::Lighter => {
            // M4: store the lighter uncle-chain extension BEFORE WASM and return
            // UncleExtended — executing it would fail pow_reward_v1 against the
            // wrong cumulative state.
            chain_state.store_competing_block(block, block.header.height)
                .map_err(|e| dwow_core::Error::Custom(format!("store competing block: {e}")))?;
            return Ok(dwow_chain::BlockConnectOutcome::UncleExtended);
        }
        dwow_chain::ReorgSignal::None => {}
    }

    // 2.5 Already-known guard — HASH-aware (sync-protocol.md §14.3). A block
    // BELOW the tip is a duplicate. A block AT the tip is a duplicate ONLY if
    // its hash matches the tip hash; a same-height block with a DIFFERENT hash
    // is a COMPETING block and MUST be stored for uncle rewards, not swallowed.
    let tip_height = chain_state.get_height();
    if block.header.height < tip_height {
        tracing::debug!(target: "block_acceptor",
            "Block {} below tip {} — duplicate, skipping", block.header.height, tip_height);
        return Ok(BlockConnectOutcome::AlreadyKnown);
    }
    if block.header.height == tip_height {
        let block_hash = chain_state.hash_block_with_cached_vm(block)
            .map_err(|e| dwow_core::Error::Custom(format!("hash block for dup check: {e}")))?;
        if chain_state.tip_hash().map(|(_, h)| h) == Some(block_hash) {
            tracing::debug!(target: "block_acceptor",
                "Block {} is the tip (duplicate) — skipping", block.header.height);
            return Ok(BlockConnectOutcome::AlreadyKnown);
        }
        // Competing block at the tip: store it via connect_block's
        // CompetingStored path (PoW/target/timestamp validation + dedup),
        // WITHOUT running WASM — pow_reward_v1 would fail against the already
        // advanced cumulative supply.
        tracing::debug!(target: "block_acceptor",
            "Block {} is a same-height competing block — storing", block.header.height);
        return chain_state.connect_block(block, uncles, None, None, None)
            .map_err(|e| dwow_core::Error::Custom(format!("store competing block: {e}")));
    }

    // 3. WASM execution — runs pow_reward_v1, persists cumulative supply chain
    // to the contracts sled tree via the overlay.
    tracing::debug!(target: "block_acceptor", "executing WASM ({} txs)...", block.transactions.len());
    let t0 = std::time::Instant::now();
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
    tracing::info!(target: "block_acceptor", "WASM execution complete ({:.1}s)", t0.elapsed().as_secs_f64());

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
        chain_state, &outcome.overlay, block)?;

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

    // Capture the contracts-tree undo BEFORE aggregation so `disconnect_block`
    // can reverse EVERY WASM contracts-tree write (not just the 3 cumulative
    // singletons). This is Bitcoin's `CBlockUndo`: the old value of every key
    // the WASM touched.
    let contracts_undo: Vec<(Vec<u8>, Option<Vec<u8>>)> = match outcome.overlay.diff(&[]) {
        Ok(diff) => {
            let mut ops = Vec::new();
            for (k, v) in diff.removed.iter() {
                ops.push((k.to_vec(), Some(v.to_vec())));
            }
            for (k, v) in diff.cache.iter() {
                match &v.0 {
                    Some(prev) => ops.push((k.to_vec(), Some(prev.to_vec()))),
                    None => ops.push((k.to_vec(), None)),
                }
            }
            ops
        }
        Err(e) => {
            return Err(dwow_core::Error::Custom(format!("contracts overlay diff: {e}")));
        }
    };
    let contracts_undo_bytes = dwow_serial::serialize(&contracts_undo);

    // 5. Aggregate WASM execution overlay into a sled batch.
    // Empty overlay is valid for blocks with no contract calls beyond coinbase.
    // The supply_chain_batch.is_none() guard above catches the critical case
    // where cumulative supply state was expected but not written.
    let contracts_batch = outcome.overlay.state.aggregate().unwrap_or_default();

    // 5.5 Coinbase maturity — HAZOP C-6 fix: enforce BEFORE sled commit.
    // Previously checked post-commit (chain_state.rs:1065) which meant
    // immature spends were persisted to disk before rejection.
    // Now enforced here in the pre-commit validation pipeline.
    for tx in &block.transactions {
        let is_coinbase = tx.contract_calls.first()
            .map_or(false, |c| c.data.first() == Some(&0x05)
                && c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID);
        if is_coinbase {
            continue;
        }
        for nullifier in &tx.nullifiers {
            if let Some(created_at) = chain_state.nullifier_height(nullifier) {
                if block.header.height.saturating_sub(created_at) < dwow_chain::COINBASE_MATURITY {
                    return Err(dwow_core::Error::Custom(format!(
                        "Immature coinbase spend at height {}: nullifier created at {}, needs {} blocks maturity",
                        block.header.height, created_at, dwow_chain::COINBASE_MATURITY
                    )));
                }
            }
        }
    }

    // 6. Atomic commit — blocks, contracts, supply_chain, consensus, commitment_set,
    // and nullifiers all committed in a single sled transaction.
    tracing::debug!(target: "block_acceptor", "committing to chain...");
    // H4: the contracts undo record is written ATOMICALLY with the block commit
    // (Bitcoin `CBlockUndo`) — passed into connect_block, which includes it in
    // the same cross-tree sled transaction. No separate post-commit write.
    let outcome = chain_state.connect_block(
        block, uncles, Some(contracts_batch), supply_chain_batch, Some(contracts_undo_bytes),
    ).map_err(|e| dwow_core::Error::Custom(format!("connect_block failed: {}", e)))?;
    tracing::debug!(target: "block_acceptor", "committed");

    // Handle reorg: if connect_block detected a heavier competing chain,
    // disconnect the displaced canonical block and re-accept both blocks.
    let final_outcome = match outcome {
        BlockConnectOutcome::ReorgAvailable { fork_height, competing_block } => {
            tracing::info!(target: "block_acceptor", "Reorg: disconnecting canonical block at height {}", fork_height);
            perform_reorg(chain_state, block, uncles, vm, target, fee_estimator, fork_height, &competing_block)?
        }
        other => {
            // 7. Update in-memory cache AFTER the atomic transaction succeeds.
            //    (For reorg, the recursive accept_block calls handle their own cache.)
            if let Some(entry) = sc_entry {
                chain_state.supply_chain.update_cache(block.header.height, entry);
            }
            other
        }
    };

    Ok(final_outcome)
}

/// Disconnect the displaced canonical block at `fork_height`, then re-accept the
/// competing block and this extension (full WASM re-execution against the
/// competing chain's state). Shared by the pre-WASM fork detection and the
/// post-`connect_block` `ReorgAvailable` outcome.
///
/// Spec: sync-protocol.md §19.3 (depth-1 reorg).
#[allow(clippy::too_many_arguments)]
fn perform_reorg(
    chain_state: &CChainState,
    block: &Block,
    uncles: &[UncleBlock],
    vm: &Arc<randomx::RandomXVM>,
    target: BlockTarget,
    fee_estimator: Option<&std::sync::Arc<dwow_chain::fee_estimator::FeeEstimator>>,
    fork_height: BlockHeight,
    competing_block: &Block,
) -> Result<BlockConnectOutcome> {
    // 1. Disconnect the displaced canonical block at fork_height.
    chain_state.disconnect_block(fork_height).map_err(|e| {
        dwow_core::Error::Custom(format!("disconnect_block({}) failed: {}", fork_height, e))
    })?;

    // 1.5 Roll back the cumulative-commit singletons to the shared prefix
    // (S_{fork_height-1}). disconnect_block deliberately does NOT touch the
    // contracts tree, so without this the competing block's pow_reward_v1
    // would read the stale S_{fork_height} and fail old_cumulative_commit.
    rollback_cumulative_commit(
        chain_state,
        fork_height.pred().unwrap_or(BlockHeight::new(0)),
    )?;

    // Remove the competing parent from `competing_blocks` before re-accepting its
    // extension, otherwise `detect_reorg` re-fires on the same parent and recurses
    // infinitely (stack overflow).
    let parent_hash = chain_state
        .hash_block_with_cached_vm(competing_block)
        .map_err(|e| dwow_core::Error::Custom(format!("Reorg: hash competing block: {}", e)))?;
    chain_state.remove_competing(fork_height, parent_hash);

    tracing::info!(target: "block_acceptor", "Reorg: disconnected H={}, accepting competing block", fork_height);

    // 2. Accept the competing block at H (full pipeline: structural, PoW, WASM, commit).
    let flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
    let rx_cache = chain_state
        .get_cache(competing_block.header.randomx_key)
        .map_err(|e| dwow_core::Error::Custom(format!("Reorg: RandomX cache: {}", e)))?;
    let comp_vm = Arc::new(
        randomx::RandomXVM::new(flags, Some(rx_cache), None)
            .map_err(|e| dwow_core::Error::Custom(format!("Reorg: competing block RandomX VM: {}", e)))?,
    );
    let comp_target = competing_block.header.target;
    let first = accept_block(
        chain_state,
        competing_block,
        &[], // competing blocks are stored without uncles
        &comp_vm,
        fork_height.pred().unwrap_or(BlockHeight::new(0)),
        comp_target,
        fee_estimator,
    )?;
    if !matches!(first, BlockConnectOutcome::CanonicalExtension { .. }) {
        return Err(dwow_core::Error::Custom(format!(
            "Reorg: competing block at {} not accepted as canonical (got {:?})",
            fork_height, first
        )));
    }

    // 3. Re-accept the current block (re-executes WASM against the competing chain).
    let second = accept_block(chain_state, block, uncles, vm, fork_height, target, fee_estimator)?;
    tracing::info!(target: "block_acceptor", "Reorg complete: chain tip at height {}", block.header.height);
    Ok(second)
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
    block: &Block,
) -> dwow_core::Result<(Option<sled::Batch>, Option<dwow_chain::CumulativeSupplyEntry>)> {
    let height = block.header.height;
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
        total_supply: SupplyAmount::new(total_supply),
    };

    // M10: host-side re-derivation — do NOT trust the WASM overlay mirror
    // blindly. Recompute S_H = S_{H-1} + C_base from the coinbase's value
    // commitment and the previous supply entry, then cross-check against what
    // WASM wrote. A divergence means the WASM overlay and the host disagree on
    // supply accounting — reject the block.
    if let Some(coinbase_params) = block.transactions.first()
        .and_then(|tx| tx.contract_calls.first())
        .filter(|c| c.data.first() == Some(&(dwow_native_token_contract::NativeTokenFunction::PoWRewardV1 as u8)))
        .and_then(|c| dwow_native_token_contract::model::PoWRewardParamsV1::decode(&c.data[1..]).ok())
    {
        let prev = chain_state.supply_chain
            .get(height.pred().unwrap_or(BlockHeight::new(0)))
            .map_err(|e| dwow_core::Error::Custom(format!("supply_chain get prev: {e}")))?;
        let expected_commit = prev.value_commit + coinbase_params.output.value_commit;
        let expected_blind = prev.blind + coinbase_params.input.value_blind.inner();
        let expected_supply = prev.total_supply.saturating_add(SupplyAmount::new(coinbase_params.input.value));
        if expected_commit != entry.value_commit
            || expected_blind != entry.blind
            || expected_supply.get() != entry.total_supply.get()
        {
            return Err(dwow_core::Error::Custom(format!(
                "cumulative supply re-derivation mismatch: host (commit={:?}, blind={:?}, supply={}) != overlay (commit={:?}, blind={:?}, supply={})",
                expected_commit, expected_blind, expected_supply.get(),
                entry.value_commit, entry.blind, entry.total_supply.get()
            )));
        }
    }

    // Build the supply_chain batch for atomic commit
    let mut batch = sled::Batch::default();
    chain_state.supply_chain.commit_to_batch(&mut batch, height, &entry)
        .map_err(|e| dwow_core::Error::Custom(format!("supply_chain commit_to_batch: {}", e)))?;

    Ok((Some(batch), Some(entry)))
}

/// Roll back the native-token cumulative-commit singletons (TOTAL_SUPPLY,
/// CUMULATIVE_VALUE_COMMIT, CUMULATIVE_BLIND) in the contracts tree to the
/// value recorded at `height` in the supply_chain tree (i.e. `S_{height}`).
///
/// Spec: sync-protocol.md §19.2/§19.3 (cumulative-commit rollback). `disconnect_block`
/// deliberately does NOT touch the contracts tree — it relies on re-execution to
/// overwrite the singletons. A reorg must therefore restore the singletons to the
/// shared-prefix value BEFORE re-connecting the competing chain, otherwise the
/// competing block's `pow_reward_v1` reads the stale `S_{tip}` and fails the
/// `old_cumulative_commit` check.
fn rollback_cumulative_commit(chain_state: &CChainState, height: BlockHeight) -> Result<()> {
    use dwow_native_token_contract::{
        NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND,
        NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT,
        NATIVE_TOKEN_CONTRACT_INFO_TREE,
        NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY,
    };
    use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

    let prev_entry = chain_state
        .supply_chain
        .get(height)
        .map_err(|e| dwow_core::Error::Custom(format!("Reorg: supply_chain get({}): {}", height, e)))?;
    let info_prefix = NATIVE_TOKEN_CONTRACT_ID.hash_state_id(NATIVE_TOKEN_CONTRACT_INFO_TREE);
    let mut rollback = sled::Batch::default();

    let mut total_key = Vec::from(info_prefix.as_slice());
    total_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY);
    rollback.insert(total_key, dwow_serial::serialize(&prev_entry.total_supply.get()));

    let mut cum_key = Vec::from(info_prefix.as_slice());
    cum_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT);
    rollback.insert(cum_key, dwow_serial::serialize(&prev_entry.value_commit));

    let mut blind_key = Vec::from(info_prefix.as_slice());
    blind_key.extend_from_slice(NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND);
    rollback.insert(blind_key, dwow_serial::serialize(&prev_entry.blind));

    chain_state
        .store
        .contracts
        .apply_batch(rollback)
        .map_err(|e| dwow_core::Error::Custom(format!("Reorg: contracts rollback: {}", e)))
}

/// General-depth reorg (Bitcoin `DisconnectBlock`/`ConnectBlock`): adopt a
/// competing chain that carries more accumulated work than the local canonical
/// chain, bounded by the shared prefix `fork_point` (both chains share blocks
/// `1..=fork_point`).
///
/// Spec: sync-protocol.md §19.2 (general-depth reorg); consensus.md §Fork Choice Rule
/// (heaviest-chain).
///
/// `competing_blocks` are the fetched competing blocks in ascending height
/// order, from `fork_point + 1` up to the extension's parent. The caller
/// re-accepts the extension block after this returns.
pub fn reorganize_to_chain(
    chain_state: &CChainState,
    competing_blocks: &[Block],
    fork_point: BlockHeight,
    fee_estimator: Option<&std::sync::Arc<dwow_chain::fee_estimator::FeeEstimator>>,
) -> Result<()> {
    // 1. Restore the cumulative-commit singletons to S_{fork_point} (the shared prefix).
    rollback_cumulative_commit(chain_state, fork_point)?;

    // 2. Disconnect canonical blocks from the tip down to fork_point + 1,
    //    stashing each removed block so a failed reconnect is diagnosable
    //    (Bitcoin keeps the disconnected block in the block index; here we
    //    re-read it from the store before removal).
    let mut disconnected: Vec<Block> = Vec::new();
    let mut h = chain_state.get_height();
    while h > fork_point {
        let block = chain_state.get_block(h).map_err(|e| {
            dwow_core::Error::Custom(format!("Reorg: get_block({}) before disconnect: {}", h, e))
        })?;
        chain_state.disconnect_block(h).map_err(|e| {
            dwow_core::Error::Custom(format!("disconnect_block({}) failed: {}", h, e))
        })?;
        disconnected.push(block);
        h = h.pred().unwrap_or(BlockHeight::new(0));
    }

    // 3. Connect the competing chain fork_point+1 ..= N in order (full pipeline:
    //    PoW, WASM re-execution against the restored cumulative state, commit).
    let flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
    for competing in competing_blocks {
        let rx_cache = match chain_state.get_cache(competing.header.randomx_key) {
            Ok(c) => c,
            Err(e) => {
                log_reorg_failure(chain_state, fork_point, &disconnected, &format!("RandomX cache: {e}"));
                return Err(dwow_core::Error::Custom(format!("Reorg: RandomX cache: {}", e)));
            }
        };
        let vm = match randomx::RandomXVM::new(flags, Some(rx_cache), None) {
            Ok(v) => Arc::new(v),
            Err(e) => {
                log_reorg_failure(chain_state, fork_point, &disconnected, &format!("RandomX VM: {e}"));
                return Err(dwow_core::Error::Custom(format!("Reorg: competing block RandomX VM: {}", e)));
            }
        };
        let pred = competing.header.height.pred().unwrap_or(BlockHeight::new(0));
        let target = competing.header.target;
        let outcome = match accept_block(chain_state, competing, &[], &vm, pred, target, fee_estimator) {
            Ok(o) => o,
            Err(e) => {
                log_reorg_failure(chain_state, fork_point, &disconnected, &format!("accept_block: {e}"));
                return Err(e);
            }
        };
        if !matches!(outcome, BlockConnectOutcome::CanonicalExtension { .. }) {
            log_reorg_failure(
                chain_state,
                fork_point,
                &disconnected,
                &format!("non-canonical outcome {outcome:?}"),
            );
            return Err(dwow_core::Error::Custom(format!(
                "Reorg: competing block at {} not accepted as canonical (got {:?})",
                competing.header.height, outcome
            )));
        }
    }
    Ok(())
}

/// Log a failed reorg so operators can see the chain is truncated and needs re-sync.
///
/// Spec: sync-protocol.md §19.2 (reorg diagnosability — never silently lose the chain).
fn log_reorg_failure(
    chain_state: &CChainState,
    fork_point: BlockHeight,
    disconnected: &[Block],
    cause: &str,
) {
    tracing::error!(
        target: "block_acceptor",
        "Reorg failed ({cause}) — chain truncated at height {}; {} disconnected block(s) need re-sync",
        fork_point,
        disconnected.len()
    );
    for b in disconnected {
        match chain_state.hash_block_with_cached_vm(b) {
            Ok(bh) => tracing::info!(target: "block_acceptor", "  disconnected block {}: {bh}", b.header.height),
            Err(_) => tracing::info!(target: "block_acceptor", "  disconnected block {}", b.header.height),
        }
    }
}
