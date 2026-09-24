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

//! Pure block validation functions.
//!
//! # Process Engineering Context — The Flow Meter
//!
//! This module is the FLOW METER for the transaction pipeline. It verifies
//! the Pedersen mass balance — the cryptographic proof that monetary mass is
//! conserved across every block:
//!
//!   Σoutputs + Σfees + Σburns == Σinputs
//!
//! The meter reading is consensus-critical: if the mass balance fails, the
//! block is rejected. Meter fraud IS hidden inflation — a forged meter reading
//! would allow a miner to mint arbitrary amounts beyond the emission schedule.
//! This is the defense-in-depth against the ZCash Orchard exploit class.
//!
//! Every function in this module is **pure**: it takes data in, returns a
//! `Result` out. No sled, no locks, no async, no side effects.
//!
//! See: `consensus.md §Supply Audit` for the flow metering specification.
//! See: `fee-spec.md §0.1` for the process engineering analogy (pipe/valve/meter).
//!
//! This makes each check independently testable with a standard `#[test]`
//! — construct minimal inputs, call the function, assert the outcome.

use std::collections::HashSet;

use blake3::Hash as Blake3Hash;
use dwow_sdk::blockchain::{BlockHeight, BlockTarget, BlockTimestamp, BlockVersion};
#[cfg(feature = "pow")]
use randomx::RandomXVM;

use super::{Block, LinearError, PowSource, Result, UncleBlock};
#[cfg(feature = "pow")]
use super::verify_uncle_proof;

/// Stage-1 PoW validation, shared by the canonical acceptance path
/// (`check_block_header`) and the competing/uncle-extension path
/// (`CChainState::validate_competing_block`).
///
/// Native blocks: the precomputed `block_hash` (RandomX, the block's own
/// key) must meet the header's declared target.
/// Monero merge-mined blocks: skip native RandomX entirely — the PoW lives
/// on the Monero side — but MUST carry a valid coinbase Merkle proof
/// (HAZOP C6).
///
/// The caller precomputes `block_hash` so the canonical path avoids a second
/// RandomX run (the same hash feeds later error messages).
///
/// UNVERIFIED(HYG-8-1): needs cargo test -p dwow_chain --test-threads=2
/// && cargo test -p dwowd --lib --test-threads=2 -- daemon_sync_integration
/// This fn is the exact stage-1 code extracted from check_block_header. The
/// competing/uncle path previously ran the native-hash check UNCONDITIONALLY
/// and only then the Monero merkle check, so merge-mined competing blocks
/// failed InvalidPoW against their own declared (native) target — diverging
/// from the canonical path and from the H-14/M-3 intent that the paths
/// match. It now uses these canonical semantics.
pub fn check_pow_stage(block: &Block, block_hash: &Blake3Hash) -> Result<()> {
    if let PowSource::Monero(monero_data) = &block.header.pow_source {
        if !monero_data.is_coinbase_valid_merkle_root() {
            return Err(LinearError::BlockIsInvalid(
                "Monero coinbase Merkle proof invalid".into()
            ));
        }

        // The claimed Monero anchor hash must be the one **derived from this block's own proof**
        // (OBL-C67). Until 2026-09-22 the field was an independent, unauthenticated claim about the same
        // block the proof describes, and nothing compared the two — so a peer could assert any Monero
        // hash it liked. A zero field means "not reported", which is what every production block does
        // (`mm_rpc.rs` builds merge-mined blocks with both anchor fields zeroed), and is left alone
        // rather than treated as a mismatch.
        //
        // Note what this does *not* establish: that the Monero block is real or on the Monero chain.
        // The three receipts prove only coinbase-inclusion in *some serialized* block, so this check
        // makes the field honest about the proof, not the proof honest about Monero. See the row.
        let claimed = block.header.anchor_monero_hash;
        if claimed != [0u8; 32] && claimed != monero_data.block_hash() {
            return Err(LinearError::BlockIsInvalid(
                "Monero anchor hash does not match the block's own proof".into(),
            ));
        }
    } else {
        // spec dispensation: type-system.md §2.3 — blake3::Hash is always
        // 32 bytes; [0..4].try_into() is provably infallible for a fixed 4-byte
        // slice of a 32-byte array.
        let b = block_hash.as_bytes();
        let hash_u32 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if !block.header.target.hash_is_valid(hash_u32) {
            return Err(LinearError::InvalidPoW(block_hash.to_string()));
        }
    }

    Ok(())
}

/// Verify a block header against all consensus rules.
///
/// Two-stage PoW validation (Bitcoin Core pattern):
///   Stage 1: `hash_u32 <= block.header.target` — hash meets header's target.
///   Stage 2: `block.header.target == expected_target` — target matches
///            consensus rules (GetNextWorkRequired). This prevents
///            self-declared-target attacks.
///
/// For genesis (height=1), `get_next_work_required(1)` returns `u32::MAX`,
/// so the declared target of `u32::MAX` passes Stage 2.
///
/// Pure — does NOT execute WASM or touch the database.
#[cfg(feature = "pow")]
pub fn check_block_header(
    block: &Block,
    vm: &RandomXVM,
    expected_target: BlockTarget,
    current_height: BlockHeight,
    previous_hash: Option<&Blake3Hash>,
) -> Result<()> {
    // Phase 0 structural: block version MUST be CURRENT.
    // Per type-system.md §2.3: BlockVersion is a nominal consensus domain.
    // Rejecting unknown versions enables future soft forks via version bits.
    // This is the cheapest possible check — fail before PoW computation.
    if block.header.version != BlockVersion::CURRENT {
        return Err(LinearError::BlockIsInvalid(format!(
            "unsupported block version: {} (current: {})",
            block.header.version, BlockVersion::CURRENT
        )));
    }

    let block_hash = block.hash_with_vm(&vm)?;

    // Stage 1: PoW — shared with the competing/uncle-extension path.
    check_pow_stage(block, &block_hash)?;

    // Height continuity: must be exactly current + 1.
    // Checked BEFORE previous hash and target — structural errors fail fast.
    if block.header.height != current_height.succ() {
        return Err(LinearError::HeightDiscontinuity {
            expected: current_height.succ(),
            got: block.header.height,
        });
    }

    // Previous hash — fork detection MUST come before Stage 2 target.
    // A block from a different fork will have the wrong previous_hash.
    // Failing here with InvalidPreviousHash is the correct diagnostic.
    // Previously this was checked AFTER Stage 2 target, causing fork blocks
    // to fail with misleading "target mismatch" errors.
    if let Some(prev) = previous_hash {
        if block.header.previous != *prev {
            return Err(LinearError::InvalidPreviousHash(block_hash.to_string()));
        }
    }

    // Merkle root
    if !block.verify_merkle_root() {
        return Err(LinearError::MerkleRootMismatch(block_hash.to_string()));
    }

    // Stage 2: The block's declared target must match what consensus rules
    // require for this height. Only reached if the block connects to our
    // canonical chain (previous hash matched above).
    if block.header.target != expected_target {
        return Err(LinearError::InvalidTarget {
            declared: block.header.target.get(),
            expected: expected_target.get(),
            height: block.header.height,
        });
    }

    Ok(())
}

/// Validate block timestamp against consensus rules.
///
/// This function is **pure** — deterministic function of block data only.
/// Per type-system.md §9, consensus validation SHALL NOT depend on wall-clock time.
///
/// The future-timestamp check (Bitcoin Core's MAX_FUTURE) is a network policy,
/// not a consensus rule. It is enforced at the P2P layer before relaying a block.
///
/// Median time warp protection (Bitcoin Core CheckBlockTimestamp pattern):
/// timestamp MUST be strictly greater than the median of the last
/// MEDIAN_BLOCK_COUNT (11) block timestamps.
pub fn check_block_timestamp(
    timestamp: BlockTimestamp,
    height: BlockHeight,
    recent_timestamps: &[BlockTimestamp],
) -> Result<()> {
    const MEDIAN_BLOCK_COUNT: usize = 11;

    // Median of last N blocks (time warp protection).
    // This is the deterministic portion of Bitcoin Core's CheckBlockTimestamp.
    // The non-deterministic future-timestamp check is a P2P policy, not a consensus rule.
    // Apply median protection when at least one prior timestamp is available.
    // For blocks 2-11 this uses whatever timestamps exist (fewer than MEDIAN_BLOCK_COUNT),
    // preventing difficulty/time manipulation during bootstrap.
    if height > BlockHeight::GENESIS && !recent_timestamps.is_empty() {
        let mut sorted: Vec<BlockTimestamp> = recent_timestamps.to_vec();
        sorted.sort_unstable();
        let median = sorted[sorted.len() / 2];
        if timestamp <= median {
            return Err(LinearError::InvalidTimestamp {
                timestamp: timestamp.get(),
                reason: format!("timestamp must be > median of last {} blocks", MEDIAN_BLOCK_COUNT),
            });
        }
    }

    Ok(())
}

/// Verify uncle blocks against all consensus rules.
///
/// Pure — the caller provides the pre-computed uncle merkle root, proofs, the
/// per-uncle targets resolved from chain state, and the set of already-stored
/// uncle keys (from the database). This function does not touch sled.
///
/// `uncle_targets[i]` MUST be the target in force at `uncles[i].header.height`
/// (resolved by the caller via `CChainState::block_target_at`), NOT the
/// referencing block's target. The target is recomputed every block from a
/// sliding timestamp window, so the current target is not the uncle's rule — see
/// the PoW comment in the loop body.
///
/// Contract (P2-3): the caller MUST have already verified the root against
/// the uncle set — block_acceptor builds the merkle from the block's uncles
/// and compares it to the header before calling in, and connect_block
/// re-checks it (H2.2) for direct callers. The redundant full re-hash
/// (build_uncle_merkle, incl. per-uncle RandomX PoW) previously done here
/// was removed; each uncle is still bound to the root individually below
/// via verify_uncle_proof.
///
/// P2-9-4: PoW is checked inside verify_uncle_proof with the uncle's OWN
/// randomx_key (uncles are mined at H-1 with K(H-1), so re-hashing here with
/// the canonical VM key K(H) would produce garbage). The dedup key matches
/// the sled `uncles`-tree key form: blake3(dwow_serialize(&header)).
/// UNVERIFIED(P2-9-4): needs cargo test -p dwow_chain && cargo test -p dwowd --lib -- uncle_minting daemon_sync_integration
#[cfg(feature = "pow")]
pub fn check_uncles(
    uncles: &[UncleBlock],
    proofs: &[super::UncleProof],
    expected_uncle_root: &[u8; 32],
    current_height: BlockHeight,
    uncle_targets: &[BlockTarget],
    existing_uncle_keys: &HashSet<[u8; 32]>,
) -> Result<()> {
    // H2.3: Reject blocks with too many uncles — prevents block bloat
    // and gas exhaustion during uncle transaction execution.
    if uncles.len() > super::MAX_UNCLE_COUNT {
        return Err(LinearError::TooManyUncles {
            count: uncles.len(),
            max: super::MAX_UNCLE_COUNT,
        });
    }

    // `uncle_targets[i]` is the target in force at `uncles[i].header.height`.
    // Both slices are built from the same uncle set by the caller; a mismatch
    // would silently skew every PoW verdict, so fail closed on it.
    if uncle_targets.len() != uncles.len() {
        return Err(LinearError::BlockIsInvalid(format!(
            "check_uncles: {} uncles but {} targets",
            uncles.len(), uncle_targets.len()
        )));
    }

    // OBL-C114: `proofs[i]` is indexed in the loop below for every uncle (the
    // slice is handed to `verify_uncle_proof` alongside `uncle_targets[i]`), so
    // a short slice is an out-of-bounds read rather than a wrong verdict. The
    // argument the guard above makes applies verbatim — both slices are built
    // from the same uncle set by the caller — and the asymmetry was the row:
    // the first was guarded, the second was not.
    //
    // `Consensus/UncleRules.lean`'s `guard_buys_every_index` is the theorem
    // this clause is: alignment is exactly what makes every in-range index of
    // one slice in range of the other, and `unaligned_has_an_uncovered_index`
    // is the case it exists to make impossible. Both are stated over `{α β}`
    // — any two lists — so the same two lemmas cover this slice and the one
    // above without restatement.
    if proofs.len() != uncles.len() {
        return Err(LinearError::BlockIsInvalid(format!(
            "check_uncles: {} uncles but {} proofs",
            uncles.len(), proofs.len()
        )));
    }

    // The base reward in force at the referencing height. The pin split is
    // carved out of THIS amount, so it is the only correct basis for deriving
    // `pin_confirmed` — not any producer-supplied figure.
    let base_reward = dwow_sdk::blockchain::expected_reward(current_height);

    for (i, uncle) in uncles.iter().enumerate() {
        // P2-9-4: dedup key = the sled `uncles`-tree key form
        // (blake3(dwow_serialize(&header)) — the commit batch in
        // chain_state.rs connect_block). The previous to_mining_blob() form
        // never matched stored keys, so the HAZOP H25 cross-block dedup
        // (stored_uncle_hashes) was inert.
        let uncle_key: [u8; 32] =
            *blake3::hash(&dwow_serial::serialize(&uncle.header)).as_bytes();

        // Depth is explicit and bounded at BOTH ends. A sibling block at the
        // SAME height as the referencing block (depth 0) is not an uncle at all,
        // and the pin schedule `base_reward / 2^depth` is defined for depth >= 1
        // only (uncle_merkle.md §Reward Distribution — the table starts at
        // depth 1 = 50%). Admitting depth 0 would pay 100% of the base reward.
        // `sat_sub` maps a block claiming a height ABOVE the referencing block
        // onto depth 0, so future-dated uncles are rejected by the same rule.
        let depth = current_height.get().saturating_sub(uncle.header.height.get());
        if depth == 0 {
            return Err(LinearError::BlockIsInvalid(format!(
                "uncle at height {} is a sibling of the referencing block at height {} — \
                 depth 0 is not an uncle",
                uncle.header.height, current_height
            )));
        }
        if depth > super::MAX_UNCLE_DEPTH as u64 {
            return Err(LinearError::UncleTooOld {
                uncle_height: uncle.header.height,
                current: current_height,
                max_depth: super::MAX_UNCLE_DEPTH,
            });
        }

        // Merkle proof against the canonical block's uncle_merkle_root.
        // verify_uncle_proof re-computes RandomX with the uncle's OWN
        // randomx_key and enforces the supplied target — the full PoW gate,
        // subsuming the former re-hash here (P2-9-4).
        //
        // The target is the one in force at the UNCLE's height, not the
        // referencing block's: the target is recomputed every block from a
        // sliding timestamp window (consensus.rs::get_next_work_required), so
        // using the current block's target would reject honest uncles mined a
        // few blocks earlier — and would make which stale work is payable a
        // function of the current target.
        if !verify_uncle_proof(&uncle.header, &proofs[i], expected_uncle_root, uncle_targets[i]) {
            return Err(LinearError::UncleProofInvalid(hex::encode(uncle_key)));
        }

        // The pin is DERIVED, not trusted: `pin_confirmed_i = base_reward / 2^depth_i`
        // (uncle_merkle.md §Reward Distribution). `pin_confirmed` is a wire field
        // the producer controls AND it sits outside `uncle_merkle_root` (which
        // commits only to the header), so without this check a relaying producer
        // could rewrite the split for any included uncle. A rejected pin
        // (`pin_accepted == false`) pays nothing, so its field is unconstrained.
        if uncle.pin_accepted {
            // `depth` is in 1..=MAX_UNCLE_DEPTH (checked above).
            let expected_pin = base_reward.split_for_uncle(depth as u8);
            if uncle.pin_confirmed != expected_pin {
                return Err(LinearError::BlockIsInvalid(format!(
                    "uncle pin_confirmed {} != base_reward({}) / 2^{} = {} — the split is \
                     derived from depth, not declared",
                    uncle.pin_confirmed, base_reward, depth, expected_pin
                )));
            }
        }

        // Uniqueness: uncle must not already be in the chain.
        if existing_uncle_keys.contains(&uncle_key) {
            return Err(LinearError::DuplicateUncle(hex::encode(uncle_key)));
        }
    }

    Ok(())
}

/// Phase 0 structural validation — cheapest checks first, fail fast.
///
/// Per formal guardrail CONSENSUS INVARIANT:
///   VALID_COINBASE(block) checks block structure before PoW, ZK, or WASM.
///
/// Checks (in order, each cheap):
///   1. Block has at least 1 transaction
///   2. First transaction has PoWRewardV1 call (contract_calls[0], function code 0x05)
///   3. Exactly one PoWRewardV1 call in the block
///   4. PoWRewardV1 call data is non-empty (params present)
///   5. FeeCollectV1 rules (consensus-coinbase.md §3.15): at most one call,
///      present iff summed FeeV3 fees > 0, and at the final position
///
/// Pure — no sled, no locks, no async, no side effects. Testable in isolation.
pub fn validate_block_structure(block: &Block) -> Result<()> {
    if block.transactions.is_empty() {
        return Err(LinearError::BlockStructure(
            "empty block — must have at least 1 transaction (coinbase)".into()
        ));
    }

    let first_has_pow = block.transactions[0].first_call_is_pow_reward();
    if !first_has_pow {
        return Err(LinearError::BlockStructure(
            "PoWRewardV1 not first — transactions[0].contract_calls[0] must carry function 0x05".into()
        ));
    }

    let pow_count = block.transactions.iter()
        .filter(|tx| tx.is_pow_reward_coinbase_tx())
        .count();
    if pow_count != 1 {
        return Err(LinearError::BlockStructure(
            format!("expected exactly 1 PoWRewardV1 call, found {}", pow_count)
        ));
    }

    // Phase 0.1b compound coinbase prevention (HAZOP F2):
    // Coinbase tx MUST have exactly 1 contract call (PoWRewardV1 only).
    // Extra calls in tx[0] would bypass Pedersen mass balance (proof_of_token_balance
    // skips entire tx[0] when first call is PoWRewardV1), ZK witness verification
    // (execution.rs skips entire tx[0]), and pre-witness checks (block_acceptor.rs
    // skips entire tx[0]). Structural fix makes call-level skip fixes defense-in-depth.
    if block.transactions[0].contract_calls.len() != 1 {
        return Err(LinearError::BlockStructure(
            "coinbase (tx[0]) must have exactly 1 contract call (PoWRewardV1 only)".into()
        ));
    }

    // Guarded by the `len() != 1` check above: contract_calls has exactly one element.
    let pow_call = &block.transactions[0].contract_calls[0];
    if pow_call.data.len() < 2 {
        return Err(LinearError::BlockStructure(
            "PoWRewardV1 call data too short — missing serialized params".into()
        ));
    }

    // Phase 0.2 contract_id check (HAZOP F7 fix):
    // Verify contract_id == NATIVE_TOKEN_CONTRACT_ID alongside function selector.
    if pow_call.contract_id != *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID {
        return Err(LinearError::BlockStructure(
            "PoWRewardV1 must target NATIVE_TOKEN_CONTRACT_ID".into()
        ));
    }

    // Phase 0.4 nullifier zero-check (HAZOP compliance fix):
    // Previously deferred to WASM Phase 4. Now enforced at structural validation
    // for fail-fast — reject blocks with zero coinbase nullifier before PoW/WASM.
    let pow_params = dwow_native_token_contract::model::PoWRewardParamsV1::decode(&pow_call.data[1..])
        .map_err(|e| LinearError::BlockStructure(format!("PoWRewardV1 params decode failed: {:?}", e)))?;
    if pow_params.nullifier.is_zero() {
        return Err(LinearError::BlockStructure(
            "coinbase nullifier is zero — must be non-zero per consensus rule".into()
        ));
    }

    // Phase 0.6 spendable-note value binding (C1) — cheap self-consistency on the
    // coinbase call, checked here so a malformed split fails before PoW/WASM.
    // Spec: uncle_merkle.md §"Spendable-note mass balance". The cross-block half
    // (`Σ pin` vs the block's uncle set) is enforced in `block_acceptor`.
    if pow_params.commitment_attrs.value != pow_params.effective_value {
        return Err(LinearError::BlockStructure(format!(
            "coinbase note preimage commits {} but effective_value is {}",
            pow_params.commitment_attrs.value, pow_params.effective_value
        )));
    }
    if pow_params.commitment_attrs.to_commitment() != pow_params.output.commitment {
        return Err(LinearError::BlockStructure(
            "coinbase commitment does not match its plaintext preimage".into()
        ));
    }
    if pow_params.effective_value.checked_add(pow_params.total_pin) != Some(pow_params.input.value) {
        return Err(LinearError::BlockStructure(format!(
            "coinbase effective_value({}) + total_pin({}) != input.value({}) — over-mint",
            pow_params.effective_value, pow_params.total_pin, pow_params.input.value
        )));
    }

    // Phase 0.5 FeeCollectV1 structural rules (consensus-coinbase.md §3.15):
    //   1. At most one FeeCollectV1 CALL per block (spec says "calls," not
    //      "transactions containing a call" — two 0x06 calls in one tx pass
    //      the old .any() check. Per-call count enforced by flat iteration.)
    //   2. FeeCollectV1 present iff the block's summed FeeV3 fees > 0
    //   3. FeeCollectV1 must be the final transaction
    // FeeV3 layout: selector 0x08 + FeeParamsV3 payload; FeeCollectV1
    // selector is 0x06. Both filtered by NATIVE_TOKEN_CONTRACT_ID.
    let is_native = |c: &crate::ContractCall| -> bool {
        c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
    };

    // Count FeeCollectV1 CALLS (not transactions) — >1 call in any tx → reject.
    let fee_collect_call_count = block.transactions.iter()
        .flat_map(|tx| tx.contract_calls.iter())
        .filter(|c| is_native(c) && c.data.first() == Some(&0x06))
        .count();
    if fee_collect_call_count > 1 {
        return Err(LinearError::BlockStructure(
            format!("expected at most 1 FeeCollectV1 call, found {}", fee_collect_call_count)
        ));
    }

    // Find the transaction containing the fee-collect call (if any) for the
    // position rule. The call count check above ensures at most one exists.
    let fee_collect_tx_position: Option<usize> = block.transactions.iter().enumerate()
        .find(|(_, tx)| tx.contract_calls.iter()
            .any(|c| is_native(c) && c.data.first() == Some(&0x06)))
        .map(|(i, _)| i);

    // Count fee calls across the block. FeeV3 replaces FeeV1 (0x00, removed).
    // FeeV3 fees are hidden behind Pedersen commitments — exact amounts are not
    // available in call data. The structural validator checks fee presence, not sum.
    // Uses typed accessor — no raw data[0] inspection.
    let fee_call_count: u64 = block.transactions.iter()
        .flat_map(|tx| &tx.contract_calls)
        .filter(|c| c.as_mass_balance_fee_v3().is_some())
        .count() as u64;

    match (fee_collect_tx_position, fee_call_count, fee_collect_call_count) {
        // Present with zero fee calls — zero-value claim / 0-fee replay (§3.13)
        (Some(_), 0, _) => {
            return Err(LinearError::BlockStructure(
                "FeeCollectV1 present but block has zero fee calls".into()
            ));
        }
        // Absent with fee calls — fees stranded permanently (§3.13)
        (None, f, _) if f > 0 => {
            return Err(LinearError::BlockStructure(
                format!("block has {} fee call(s) but no FeeCollectV1 call", f)
            ));
        }
        // Present with fees — must be the final transaction (§3.1)
        (Some(pos), _, _) => {
            if pos != block.transactions.len() - 1 {
                return Err(LinearError::BlockStructure(
                    format!(
                        "FeeCollectV1 at position {} — must be the final transaction ({})",
                        pos, block.transactions.len() - 1
                    )
                ));
            }
        }
        // Absent with zero fees — valid zero-fee block (§3.13)
        (None, _, _) => {}
    }

    Ok(())
}

#[cfg(all(test, feature = "pow"))]
mod tests {
    use super::*;
    use crate::block::build_uncle_merkle;
    use crate::fee_window::FeeWindowFlags;
    use dwow_sdk::blockchain::{BlockReward, BlockTarget, BlockVersion, MoneroBlockHeight};

    /// A block with correct defaults for empty transactions.
    /// merkle_root for 0 txs is blake3::hash(&[]).
    fn dummy_block() -> Block {
        Block {
            header: super::super::BlockHeader {
                version: BlockVersion::CURRENT,
                previous: Blake3Hash::from([0u8; 32]),
                merkle_root: blake3::hash(&[]), // correct for 0 transactions
                timestamp: BlockTimestamp::new(0),
                target: BlockTarget::MAX,
                nonce: 0,
                height: BlockHeight::new(1),
                uncle_merkle_root: [0u8; 32],
                total_reward: BlockReward::ZERO,
                randomx_key: [0u8; 32],
                miner: [0u8; 32],
                commitment_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
                pow_source: PowSource::Native,
                anchor_owner: [0u8; 32],
                caribina_anchor: None,
            },
            transactions: vec![],
        }
    }

    /// Create a VM suitable for tests using the recommended flags.
    fn test_vm() -> randomx::RandomXVM {
        let flags = randomx::RandomXFlags::get_recommended_flags();
        let cache = randomx::RandomXCache::new(flags, &[0u8; 32]).unwrap();
        randomx::RandomXVM::new(flags, Some(cache), None).unwrap()
    }

    /// OBL-C67 — a block's claimed Monero anchor hash must be the one derived from its own proof.
    ///
    /// The field was an independent, unauthenticated claim about the same block the proof describes, and
    /// nothing compared the two, so a peer could assert any Monero hash it liked. Three assertions,
    /// because any two of them alone are satisfied by a rule that is wrong in the third direction:
    ///
    ///   * absent is accepted — production writes zero, and that must keep working;
    ///   * the derived value is accepted — the honest claim;
    ///   * any other value is rejected — the dishonest one, including plausible-looking values.
    ///
    /// The fixture is the real Monero testnet block, because a check that derives a hash is only
    /// meaningful against a proof that is genuine.
    #[test]
    fn claimed_monero_anchor_hash_must_equal_the_derived_one() {
        let pow_data = crate::monero::tests::real_block_powdata();
        let derived = pow_data.block_hash();

        let mut block = dummy_block();
        block.header.pow_source = PowSource::Monero(pow_data);

        // Absent: the production case.
        block.header.anchor_monero_hash = [0u8; 32];
        assert!(
            check_pow_stage(&block, &Blake3Hash::from([0u8; 32])).is_ok(),
            "an absent claim must be accepted — every production merge-mined block carries one"
        );

        // The control that makes the rejection meaningful: the derived value must differ from zero,
        // or "rejected" below would only be testing the absent case.
        assert_ne!(derived, [0u8; 32], "the derived Monero block id must not be all-zero");

        // Honest: exactly the derived value.
        block.header.anchor_monero_hash = derived;
        assert!(
            check_pow_stage(&block, &Blake3Hash::from([0u8; 32])).is_ok(),
            "the derived value is the honest claim and must be accepted"
        );

        // Dishonest: anything else.
        for wrong in [[0xABu8; 32], [0x01u8; 32], [0xFFu8; 32]] {
            block.header.anchor_monero_hash = wrong;
            assert!(
                check_pow_stage(&block, &Blake3Hash::from([0u8; 32])).is_err(),
                "a claimed Monero anchor hash that disagrees with the block's own proof must be rejected"
            );
        }
    }

    #[test]
    fn rejects_height_discontinuity_forward() {
        let mut block = dummy_block();
        block.header.height = BlockHeight::new(5); // claim 5 when chain is at 0 — expected 1
        let err = check_block_header(
            &block,
            &test_vm(),
            BlockTarget::MAX, // expected_target (matches block.header.target = u32::MAX)
            BlockHeight::new(0), // current_height
            None,      // no previous (genesis-like)
        ).unwrap_err();
        match err {
            LinearError::HeightDiscontinuity { expected, got } => {
                assert_eq!(expected, BlockHeight::new(1));
                assert_eq!(got, BlockHeight::new(5));
            }
            e => panic!("wrong error variant: {:?}", e),
        }
    }

    #[test]
    fn rejects_height_discontinuity_backwards() {
        let block = dummy_block();
        let err = check_block_header(
            &block,
            &test_vm(),
            BlockTarget::MAX, // expected_target (must match block.header.target = u32::MAX)
            BlockHeight::new(5), // current_height=5, so expected=6, but block says 1
            None,
        ).unwrap_err();
        match err {
            LinearError::HeightDiscontinuity { expected, got } => {
                assert_eq!(expected, BlockHeight::new(6));
                assert_eq!(got, BlockHeight::new(1));
            }
            e => panic!("wrong error variant: {:?}", e),
        }
    }

    /// Stage 2 PoW: a block mined with u32::MAX target at height > 1
    /// must be rejected because the consensus target is lower.
    #[test]
    fn rejects_target_mismatch_above_genesis() {
        let block = dummy_block();
        // Block claims target=u32::MAX but consensus says 0x0FFFFFFF.
        // current_height=0 so the block at height=1 passes the height
        // continuity check (expected=1, got=1) and reaches Stage 2 target.
        let err = check_block_header(
            &block,
            &test_vm(),
            BlockTarget::new(0x0FFF_FFFF), // expected_target (must differ from block.header.target)
            BlockHeight::new(0), // current_height=0 (pre-genesis)
            None,
        ).unwrap_err();
        match err {
            LinearError::InvalidTarget { declared, expected, height } => {
                assert_eq!(declared, BlockTarget::MAX.get());
                assert_eq!(expected, 0x0FFF_FFFF);
                assert_eq!(height, BlockHeight::new(1)); // block header height
            }
            e => panic!("wrong error variant: {:?}", e),
        }
    }

    /// Stage 2 PoW: a block with matching target and u32::MAX (guaranteed
    /// PoW pass) succeeds validation when merkle root is correct.
    #[test]
    fn accepts_matching_target_and_pow() {
        let mut block = dummy_block();
        block.header.target = BlockTarget::MAX;
        block.header.height = BlockHeight::new(2);
        // expected_target = u32::MAX matches header target → stage 2 passes
        // hash_u32 <= u32::MAX → stage 1 always passes
        // merkle_root = blake3::hash(&[]) matches 0 transactions → passes
        let result = check_block_header(
            &block,
            &test_vm(),
            BlockTarget::MAX,    // expected_target matches block.header.target
            BlockHeight::new(1), // current_height=1, expected height=2
            None,
        );
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }

    // ================================================================
    // Phase 0.5 — FeeCollectV1 structural rules (consensus-coinbase.md §3.15)
    // ================================================================

    use dwow_native_token_contract::model::{
        ClearInput, Commitment, Nullifier as NtNullifier, Output, PoWRewardParamsV1 as PowParams,
    };
    use dwow_sdk::crypto::pasta_prelude::Group;
    use dwow_sdk::crypto::{note::AeadEncryptedNote, BaseBlind, Blind, FuncId, Keypair};
    use dwow_sdk::pasta::pallas;

    /// A structurally valid coinbase transaction: PoWRewardV1 call with
    /// deserializable params and a non-zero nullifier.
    fn coinbase_tx() -> crate::Transaction {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        let commitment = Commitment::from_attributes(
            &keypair.public,
            1000,
            dwow_native_token_contract::model::DRKW_ASSET_ID,
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );
        let params = PowParams {
            input: ClearInput {
                value: 1000,
                asset_id: dwow_native_token_contract::model::DRKW_ASSET_ID.inner(),
                value_blind: Blind(pallas::Scalar::zero()),
                token_blind: BaseBlind::ZERO,
                signature_public: keypair.public,
            },
            total_pin: 0,
            effective_value: 1000,
            commitment_attrs: dwow_native_token_contract::model::CommitmentAttributes {
                version: 0,
                public_key: keypair.public,
                value: 1000,
                asset_id: dwow_native_token_contract::model::DRKW_ASSET_ID,
                spend_hook: FuncId::none(),
                user_data: pallas::Base::zero(),
                blind: Blind(pallas::Base::zero()),
            },
            output: Output {
                value_commit: pallas::Point::identity(),
                token_commit: pallas::Base::zero(),
                commitment,
                nullifier: Some(NtNullifier::from_bytes([2u8; 32]).unwrap()),
                note: AeadEncryptedNote { ciphertext: vec![0u8; 32], ephem_public: keypair.public },
            },
            nullifier: NtNullifier::from_bytes([2u8; 32]).unwrap(),
            expected_cumulative_supply: 0,
            old_cumulative_commit: pallas::Point::identity(),
            old_cumulative_blind: pallas::Scalar::zero(),
            new_cumulative_commit: pallas::Point::identity(),
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };
        let mut data = vec![0x05u8];
        data.extend(dwow_serial::serialize(&params));
        crate::Transaction {
            version: BlockVersion::CURRENT,
            contract_calls: vec![crate::ContractCall {
                contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                data,
            }],
            ..Default::default()
        }
    }

    /// A FeeV3 transaction — the structural validator counts fee presence via
    /// selector `0x08` (FeeV3 replaces the removed FeeV1 `0x00`). The FeeParamsV3
    /// payload is opaque here; `as_mass_balance_fee_v3()` only checks the selector
    /// and a ≥444-byte length, so a zero-filled payload is sufficient.
    fn fee_tx(_fee: u64) -> crate::Transaction {
        let mut data = vec![0x08u8];
        data.extend_from_slice(&vec![0u8; 443]); // opaque FeeParamsV3 payload
        crate::Transaction {
            version: BlockVersion::CURRENT,
            contract_calls: vec![crate::ContractCall {
                contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                data,
            }],
            ..Default::default()
        }
    }

    /// A FeeCollectV1 transaction — Phase 0 only reads the selector.
    fn fee_collect_tx() -> crate::Transaction {
        crate::Transaction {
            version: BlockVersion::CURRENT,
            contract_calls: vec![crate::ContractCall {
                contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                data: vec![0x06u8, 0u8],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn phase05_accepts_fees_with_final_fee_collect() {
        let mut block = dummy_block();
        block.transactions = vec![coinbase_tx(), fee_tx(1), fee_collect_tx()];
        assert!(validate_block_structure(&block).is_ok());
    }

    #[test]
    fn phase05_accepts_zero_fee_block_without_fee_collect() {
        let mut block = dummy_block();
        block.transactions = vec![coinbase_tx()];
        assert!(validate_block_structure(&block).is_ok());
    }

    #[test]
    fn phase05_rejects_duplicate_fee_collect() {
        let mut block = dummy_block();
        block.transactions =
            vec![coinbase_tx(), fee_tx(1), fee_collect_tx(), fee_collect_tx()];
        let err = validate_block_structure(&block).unwrap_err();
        assert!(format!("{:?}", err).contains("at most 1 FeeCollectV1"), "got {:?}", err);
    }

    #[test]
    fn phase05_rejects_fee_collect_with_zero_fees() {
        // 0-fee replay shape (audit finding D12): FeeCollect present, no fees.
        let mut block = dummy_block();
        block.transactions = vec![coinbase_tx(), fee_collect_tx()];
        let err = validate_block_structure(&block).unwrap_err();
        assert!(format!("{:?}", err).contains("zero fee calls"), "got {:?}", err);
    }

    #[test]
    fn phase05_rejects_fees_without_fee_collect() {
        // Stranded-fees shape (spec §3.13): fees paid, no collection plate.
        let mut block = dummy_block();
        block.transactions = vec![coinbase_tx(), fee_tx(7)];
        let err = validate_block_structure(&block).unwrap_err();
        assert!(format!("{:?}", err).contains("no FeeCollectV1"), "got {:?}", err);
    }

    #[test]
    fn phase05_rejects_fee_collect_not_final() {
        let mut block = dummy_block();
        block.transactions = vec![coinbase_tx(), fee_collect_tx(), fee_tx(7)];
        let err = validate_block_structure(&block).unwrap_err();
        assert!(format!("{:?}", err).contains("final transaction"), "got {:?}", err);
    }

    #[test]
    fn phase05_short_fee_call_not_counted() {
        // A short FeeV3 call (selector 0x08, <444 bytes) is not counted as a fee —
        // the length check lives in MassBalanceFeeV3CallData::from_bytes (min 444),
        // so the block validates as a zero-fee block.
        let mut block = dummy_block();
        let mut tx = fee_tx(1);
        tx.contract_calls[0].data = vec![0x08u8, 1, 2]; // < 444 bytes
        block.transactions = vec![coinbase_tx(), tx];
        assert!(validate_block_structure(&block).is_ok());
    }

    #[test]
    fn phase05_rejects_two_collect_calls_in_one_tx() {
        // Two FeeCollectV1 calls inside ONE transaction — per-call counting
        // catches this (spec says "at most one call", not "at most one tx").
        let mut block = dummy_block();
        let mut tx = fee_collect_tx();
        // Second call in same tx, same contract_id
        tx.contract_calls.push(crate::ContractCall {
            contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
            data: vec![0x06u8, 0u8],
        });
        block.transactions = vec![coinbase_tx(), fee_tx(1), tx];
        let err = validate_block_structure(&block).unwrap_err();
        assert!(format!("{:?}", err).contains("at most 1 FeeCollectV1"), "got {:?}", err);
    }

    #[test]
    fn phase05_ignores_other_contracts_zero_selector() {
        // contract_id filter: a non-native call with the 0x08 fee selector is NOT a fee.
        let mut block = dummy_block();
        let mut alien = fee_tx(u64::MAX); // would trigger rules if counted
        alien.contract_calls[0].contract_id =
            dwow_sdk::crypto::ContractId::from_bytes([9u8; 32]).unwrap();
        block.transactions = vec![coinbase_tx(), alien];
        // No native fees, no FeeCollect → valid zero-fee block.
        assert!(validate_block_structure(&block).is_ok());
    }

    // ================================================================
    // F2: tx[0] structural — compound coinbase prevention
    // ================================================================

    /// HAZOP F2: coinbase tx (index 0) must have exactly 1 contract call.
    /// A compound coinbase would bypass Pedersen mass balance,
    /// ZK witness, and pre-witness checks.
    #[test]
    fn rejects_compound_coinbase_two_calls() {
        let mut block = dummy_block();
        let mut tx = coinbase_tx();
        // Add a second contract call to the coinbase tx
        let second_call = crate::ContractCall {
            contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
            data: vec![0x00u8, 1, 0, 0, 0, 0, 0, 0, 0],
        };
        tx.contract_calls.push(second_call);
        block.transactions = vec![tx];
        let err = validate_block_structure(&block).unwrap_err();
        match err {
            LinearError::BlockStructure(msg) => {
                assert!(
                    msg.contains("exactly 1 contract call"),
                    "expected 'exactly 1 contract call', got: {}", msg
                );
            }
            e => panic!("wrong error variant: {:?}", e),
        }
    }

    /// HAZOP F2: a valid coinbase with exactly 1 contract call must pass.
    #[test]
    fn accepts_coinbase_with_single_call() {
        let mut block = dummy_block();
        block.transactions = vec![coinbase_tx()];
        assert!(validate_block_structure(&block).is_ok());
    }

    // ================================================================
    // F5: Uncle validation — check_uncles() integration tests
    // ================================================================

    /// Build a minimal uncle block for testing. target = u32::MAX ensures
    /// any RandomX hash passes PoW.
    fn dummy_uncle(height: u64, nonce: u32) -> UncleBlock {
        UncleBlock {
            transactions: vec![],
            pin_accepted: false,
            // `pin_accepted == false`, so the pin is not payable and
            // `check_uncles` deliberately does not constrain it. An ACCEPTED pin
            // is re-derived as `base_reward(H) / 2^depth` (C4).
            pin_confirmed: BlockReward::new(0),
            header: super::super::BlockHeader {
                version: BlockVersion::CURRENT,
                previous: Blake3Hash::from([0u8; 32]),
                merkle_root: blake3::hash(&[]),
                timestamp: BlockTimestamp::new(0),
                target: BlockTarget::MAX,
                nonce,
                height: BlockHeight::new(height),
                uncle_merkle_root: [0u8; 32],
                total_reward: BlockReward::ZERO,
                randomx_key: [0u8; 32],
                miner: [0u8; 32],
                commitment_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
                pow_source: PowSource::Native,
                anchor_owner: [0u8; 32],
                caribina_anchor: None,
            },
        }
    }

    /// B1: 7 uncles → TooManyUncles (MAX_UNCLE_COUNT = 6)
    #[test]
    fn check_uncles_rejects_too_many() {
        let uncles: Vec<UncleBlock> = (0..7).map(|i| dummy_uncle(2, i)).collect();
        let (root, proofs) = build_uncle_merkle(&uncles);
        let err = check_uncles(
            &uncles, &proofs, &root,
            BlockHeight::new(10), &[BlockTarget::MAX], &std::collections::HashSet::new(),
        ).unwrap_err();
        match err {
            LinearError::TooManyUncles { count, max } => {
                assert_eq!(count, 7);
                assert_eq!(max, 6);
            }
            e => panic!("expected TooManyUncles, got {:?}", e),
        }
    }

    /// B2: Duplicate uncle → DuplicateUncle
    #[test]
    fn check_uncles_rejects_duplicate() {
        let uncle = dummy_uncle(8, 42);
        let (root, proofs) = build_uncle_merkle(&[uncle.clone()]);
        // P2-9-4: the dedup key is the sled `uncles`-tree form
        // blake3(dwow_serialize(&header)) — exactly what connect_block inserts.
        let key = *blake3::hash(&dwow_serial::serialize(&uncle.header)).as_bytes();
        let mut existing: std::collections::HashSet<[u8; 32]> = std::collections::HashSet::new();
        existing.insert(key);
        let err = check_uncles(
            &[uncle], &proofs, &root,
            BlockHeight::new(10), &[BlockTarget::MAX], &existing,
        ).unwrap_err();
        match err {
            LinearError::DuplicateUncle(_) => {}
            e => panic!("expected DuplicateUncle, got {:?}", e),
        }
    }

    /// B3: Uncle with impossible target (0) → UncleProofInvalid.
    /// P2-9-4: with the wrong-VM re-hash removed, the only PoW gate is
    /// verify_uncle_proof step 2 (own-key hash vs caller target), so the
    /// rejection surfaces as UncleProofInvalid.
    #[test]
    fn check_uncles_rejects_impossible_target() {
        let mut uncle = dummy_uncle(8, 0);
        uncle.header.target = BlockTarget::new(0); // impossible to satisfy
        let (root, proofs) = build_uncle_merkle(&[uncle.clone()]);
        let err = check_uncles(
            &[uncle], &proofs, &root,
            BlockHeight::new(10), &[BlockTarget::new(0)], &std::collections::HashSet::new(),
        ).unwrap_err();
        match err {
            LinearError::UncleProofInvalid(_) => {}
            e => panic!("expected UncleProofInvalid, got {:?}", e),
        }
    }

    /// B4: Fabricated uncle (proof/root mismatch) → UncleProofInvalid.
    /// P2-3: the full-merkle recompute against the root was removed from
    /// check_uncles (root completeness is the caller's check — see the fn
    /// contract), so a proof from tree A verified against root B must now
    /// fail at the per-uncle proof check.
    #[test]
    fn check_uncles_rejects_mismatched_proof_root() {
        let uncle_a = dummy_uncle(8, 100);
        let uncle_b = dummy_uncle(8, 200);
        let (_root_a, proofs_a) = build_uncle_merkle(&[uncle_a.clone()]);
        let (root_b, _) = build_uncle_merkle(&[uncle_b]);
        // Use proof from tree A with root from tree B → proof verification fails
        let err = check_uncles(
            &[uncle_a], &proofs_a, &root_b,
            BlockHeight::new(10), &[BlockTarget::MAX], &std::collections::HashSet::new(),
        ).unwrap_err();
        match err {
            LinearError::UncleProofInvalid(_) => {}
            e => panic!("expected UncleProofInvalid, got {:?}", e),
        }
    }

    /// B5: Uncle depth > MAX_UNCLE_DEPTH (6) → UncleTooOld
    #[test]
    fn check_uncles_rejects_too_old() {
        let uncle = dummy_uncle(2, 42); // uncle at height 2
        let (root, proofs) = build_uncle_merkle(&[uncle.clone()]);
        let current = BlockHeight::new(2 + 6 + 1); // depth = 7 > MAX_UNCLE_DEPTH
        let err = check_uncles(
            &[uncle], &proofs, &root,
            current, &[BlockTarget::MAX], &std::collections::HashSet::new(),
        ).unwrap_err();
        match err {
            LinearError::UncleTooOld { uncle_height, current: cur, max_depth } => {
                assert_eq!(uncle_height, BlockHeight::new(2));
                assert_eq!(cur, BlockHeight::new(9));
                assert_eq!(max_depth, 6);
            }
            e => panic!("expected UncleTooOld, got {:?}", e),
        }
    }

    /// B6: Valid uncle within bounds → accepted
    #[test]
    fn check_uncles_accepts_valid_uncle() {
        let uncle = dummy_uncle(8, 42);
        let (root, proofs) = build_uncle_merkle(&[uncle.clone()]);
        let result = check_uncles(
            &[uncle], &proofs, &root,
            BlockHeight::new(10), &[BlockTarget::MAX], &std::collections::HashSet::new(),
        );
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }

    /// P2-9-4 regression: an uncle mined with its OWN randomx_key — the real
    /// H-1 mining setup (Miner::mine derives the key from the uncle's height)
    /// — must pass check_uncles. The removed wrong-VM re-hash used the
    /// canonical key K(H) and returned the now-removed `UnclePoWInvalid` for
    /// exactly these uncles; the all-zero-key + BlockTarget::MAX fixtures
    /// masked it.
    #[test]
    fn check_uncles_accepts_uncle_mined_with_its_own_key() {
        let target = BlockTarget::new(0x0FFF_FFFF); // ~16 expected nonce tries
        let mut uncle = dummy_uncle(8, 0);
        uncle.header.randomx_key = [7u8; 32];
        uncle.header.target = target;

        // Mine with the uncle's own key — mirrors Miner::mine at H-1.
        let flags = randomx::RandomXFlags::get_recommended_flags();
        let cache = randomx::RandomXCache::new(flags, &uncle.header.randomx_key).unwrap();
        let vm = randomx::RandomXVM::new(flags, Some(cache), None).unwrap();
        let mut nonce: u32 = 0;
        let mined = loop {
            assert!(nonce < 100_000, "no valid nonce found in 100k tries");
            let mut candidate = uncle.clone();
            candidate.header.nonce = nonce;
            let hash = vm.calculate_hash(&candidate.header.to_mining_blob()).unwrap();
            let mut pow = [0u8; 32];
            pow.copy_from_slice(&hash[..32]);
            let hash_u32 = u32::from_le_bytes([pow[0], pow[1], pow[2], pow[3]]);
            if target.hash_is_valid(hash_u32) {
                break candidate;
            }
            nonce += 1;
        };

        let (root, proofs) = build_uncle_merkle(&[mined.clone()]);
        let result = check_uncles(
            &[mined], &proofs, &root,
            BlockHeight::new(10), &[target], &std::collections::HashSet::new(),
        );
        assert!(result.is_ok(), "uncle mined with its own key must pass: {:?}", result);
    }

    /// C7: a sibling block at the SAME height as the referencing block is not an
    /// uncle. `split_for_uncle(depth)` is defined for depth >= 1 only
    /// (uncle_merkle.md §Reward Distribution — the table starts at 50%), so
    /// admitting depth 0 would pay 100% of the base reward.
    #[test]
    fn check_uncles_rejects_depth_zero() {
        let uncle = dummy_uncle(10, 7); // same height as the referencing block
        let (root, proofs) = build_uncle_merkle(&[uncle.clone()]);
        let err = check_uncles(
            &[uncle], &proofs, &root,
            BlockHeight::new(10), &[BlockTarget::MAX], &std::collections::HashSet::new(),
        ).unwrap_err();
        match err {
            LinearError::BlockIsInvalid(msg) => {
                assert!(msg.contains("depth 0"), "unexpected message: {msg}");
            }
            e => panic!("expected BlockIsInvalid for a depth-0 uncle, got {e:?}"),
        }
    }

    /// C4: an accepted pin is DERIVED — it must equal
    /// `base_reward(H) / 2^depth`. `pin_confirmed` is a producer-controlled wire
    /// field that sits OUTSIDE `uncle_merkle_root` (which commits only to the
    /// header), so without this check a relaying producer could rewrite the split.
    #[test]
    fn check_uncles_derives_and_enforces_pin() {
        let current = BlockHeight::new(10);
        let base = dwow_sdk::blockchain::expected_reward(current);

        // Depth-2 uncle carrying the correctly derived pin → accepted.
        let mut honest = dummy_uncle(8, 42);
        honest.pin_accepted = true;
        honest.pin_confirmed = base.split_for_uncle(2);
        let (root, proofs) = build_uncle_merkle(&[honest.clone()]);
        assert!(
            check_uncles(
                &[honest], &proofs, &root, current, &[BlockTarget::MAX],
                &std::collections::HashSet::new(),
            ).is_ok(),
            "depth-2 uncle with the derived pin must be accepted"
        );

        // The same uncle with the pin rewritten to the full base → rejected.
        let mut rewritten = dummy_uncle(8, 42);
        rewritten.pin_accepted = true;
        rewritten.pin_confirmed = base;
        let (root, proofs) = build_uncle_merkle(&[rewritten.clone()]);
        let err = check_uncles(
            &[rewritten], &proofs, &root, current, &[BlockTarget::MAX],
            &std::collections::HashSet::new(),
        ).unwrap_err();
        match err {
            LinearError::BlockIsInvalid(msg) => {
                assert!(msg.contains("pin_confirmed"), "unexpected message: {msg}");
            }
            e => panic!("expected BlockIsInvalid for a rewritten pin, got {e:?}"),
        }
    }

    /// C3: the target slice is applied PER UNCLE — `uncle_targets[i]` governs
    /// `uncles[i]`. A shared "one target for all" value (the previous
    /// behaviour: the referencing block's target) cannot express the rule that
    /// each uncle is held to the target of its own height.
    #[test]
    fn check_uncles_applies_target_per_uncle() {
        let a = dummy_uncle(8, 11);
        let b = dummy_uncle(9, 22);
        let (root, proofs) = build_uncle_merkle(&[a.clone(), b.clone()]);
        let set = std::collections::HashSet::new();

        // Each uncle satisfies the (max) target at its own height → accepted.
        assert!(
            check_uncles(
                &[a.clone(), b.clone()], &proofs, &root,
                BlockHeight::new(10), &[BlockTarget::MAX, BlockTarget::MAX], &set,
            ).is_ok(),
            "both uncles satisfy their own targets"
        );

        // The SECOND uncle's target made impossible → rejected, proving the
        // slice is indexed per uncle rather than taken from a single shared value.
        let err = check_uncles(
            &[a, b], &proofs, &root,
            BlockHeight::new(10), &[BlockTarget::MAX, BlockTarget::new(0)], &set,
        ).unwrap_err();
        assert!(
            matches!(err, LinearError::UncleProofInvalid(_)),
            "expected UncleProofInvalid, got {err:?}"
        );
    }

    /// C3: a target slice that does not line up with the uncle set must fail
    /// closed rather than silently skipping a PoW verdict.
    #[test]
    fn check_uncles_rejects_misaligned_target_slice() {
        let uncle = dummy_uncle(8, 42);
        let (root, proofs) = build_uncle_merkle(&[uncle.clone()]);
        let err = check_uncles(
            &[uncle], &proofs, &root,
            BlockHeight::new(10), &[], &std::collections::HashSet::new(),
        ).unwrap_err();
        match err {
            LinearError::BlockIsInvalid(msg) => {
                assert!(msg.contains("targets"), "unexpected message: {msg}");
            }
            e => panic!("expected BlockIsInvalid for a misaligned target slice, got {e:?}"),
        }
    }

    /// C114: and the *other* slice the loop indexes by the same index must fail
    /// closed too — `proofs[i]` is read for every uncle, so a short slice is an
    /// out-of-bounds read, not a verdict.
    ///
    /// The targets are aligned here, which is what makes this the proof slice's
    /// test rather than a repeat of the one above: the case that reaches the
    /// second guard is the one the first guard passes.
    #[test]
    fn check_uncles_rejects_misaligned_proof_slice() {
        let a = dummy_uncle(8, 42);
        let b = dummy_uncle(9, 43);
        let (root, proofs) = build_uncle_merkle(&[a.clone(), b.clone()]);
        assert_eq!(proofs.len(), 2, "the builder emits one proof per uncle");

        // Two uncles, one proof, a full target slice: before the guard this
        // indexed `proofs[1]` and panicked.
        let err = check_uncles(
            &[a, b], &proofs[..1], &root,
            BlockHeight::new(10), &[BlockTarget::MAX, BlockTarget::MAX],
            &std::collections::HashSet::new(),
        ).unwrap_err();
        match err {
            LinearError::BlockIsInvalid(msg) => {
                assert!(msg.contains("proofs"), "unexpected message: {msg}");
                assert!(msg.contains("2 uncles but 1 proofs"), "unexpected message: {msg}");
            }
            e => panic!("expected BlockIsInvalid for a misaligned proof slice, got {e:?}"),
        }
    }
}
