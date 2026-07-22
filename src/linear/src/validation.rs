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
//! Every function in this module is **pure**: it takes data in, returns a
//! `Result` out. No sled, no locks, no async, no side effects.
//!
//! This makes each check independently testable with a standard `#[test]`
//! — construct minimal inputs, call the function, assert the outcome.

use std::collections::HashSet;

use blake3::Hash as Blake3Hash;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, BlockTimestamp, MoneroBlockHeight};
use randomx::RandomXVM;

use super::{
    build_uncle_merkle, verify_uncle_proof, Block, LinearError, PowSource, Result, UncleBlock,
};

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
pub fn check_block_header(
    block: &Block,
    vm: &RandomXVM,
    expected_target: BlockTarget,
    current_height: BlockHeight,
    previous_hash: Option<&Blake3Hash>,
) -> Result<()> {
    let block_hash = block.hash_with_vm(&vm);

    // Stage 1: PoW — hash must meet the block header's own target.
    // Monero merge-mined blocks skip native RandomX check.
    if !matches!(block.header.pow_source, PowSource::Monero(_)) {
        let hash_u32 = u32::from_le_bytes(block_hash.as_bytes()[0..4].try_into().unwrap());
        if hash_u32 > block.header.target.get() {
            return Err(LinearError::InvalidPoW(block_hash.to_string()));
        }
    }

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
    if height > BlockHeight::GENESIS && recent_timestamps.len() >= MEDIAN_BLOCK_COUNT {
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
/// Pure — the caller provides the pre-computed uncle merkle root,
/// proofs, and the set of already-stored uncle keys (from the database).
/// This function does not touch sled.
pub fn check_uncles(
    uncles: &[UncleBlock],
    proofs: &[super::UncleProof],
    expected_uncle_root: &[u8; 32],
    current_height: BlockHeight,
    vm: &RandomXVM,
    target: u32,
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

    // Verify the uncle merkle root matches
    let (computed_root, _) = build_uncle_merkle(uncles, vm);
    if computed_root != *expected_uncle_root {
        return Err(LinearError::UncleMerkleRootMismatch(
            hex::encode(expected_uncle_root),
        ));
    }

    for (i, uncle) in uncles.iter().enumerate() {
        let uncle_hash = uncle.hash_with_vm(&vm);

        // PoW for this uncle
        let hash_u32 = u32::from_le_bytes(uncle_hash.as_bytes()[0..4].try_into().unwrap());
        if hash_u32 > target {
            return Err(LinearError::UnclePoWInvalid(uncle_hash.to_string()));
        }

        // Merkle proof against the canonical block's uncle_merkle_root
        if !verify_uncle_proof(&proofs[i], expected_uncle_root, vm, target) {
            return Err(LinearError::UncleProofInvalid(uncle_hash.to_string()));
        }

        // Recency: uncle must not be too old
        let min_allowed = current_height.get().saturating_sub(super::MAX_UNCLE_DEPTH as u64);
        if uncle.header.height.get() <= min_allowed {
            return Err(LinearError::UncleTooOld {
                uncle_height: uncle.header.height,
                current: current_height,
                max_depth: super::MAX_UNCLE_DEPTH,
            });
        }

        // Uniqueness: uncle must not already be in the chain.
        // Uses to_mining_blob() for the same canonical representation as
        // build_uncle_merkle() and verify_uncle_proof() — consensus-coinbase.md §4.
        let uncle_key: [u8; 32] =
            *blake3::hash(&uncle.header.to_mining_blob()).as_bytes();
        if existing_uncle_keys.contains(&uncle_key) {
            return Err(LinearError::DuplicateUncle(uncle_hash.to_string()));
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
///      present iff summed FeeV1 fees > 0, and at the final position
///
/// Pure — no sled, no locks, no async, no side effects. Testable in isolation.
pub fn validate_block_structure(block: &Block) -> Result<()> {
    if block.transactions.is_empty() {
        return Err(LinearError::BlockStructure(
            "empty block — must have at least 1 transaction (coinbase)".into()
        ));
    }

    let first_has_pow = block.transactions[0].contract_calls.first()
        .map_or(false, |c| c.data.first() == Some(&0x05));
    if !first_has_pow {
        return Err(LinearError::BlockStructure(
            "PoWRewardV1 not first — transactions[0].contract_calls[0] must carry function 0x05".into()
        ));
    }

    let pow_count = block.transactions.iter()
        .filter(|tx| tx.contract_calls.first()
            .map_or(false, |c| c.data.first() == Some(&0x05)
                && c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID))
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

    let pow_call = block.transactions[0].contract_calls.first().unwrap();
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
    let pow_params: dwow_native_token_contract::model::PoWRewardParamsV1 =
        dwow_serial::deserialize(&pow_call.data[1..])
            .map_err(|e| LinearError::BlockStructure(
                format!("PoWRewardV1 params deserialization failed: {}", e)
            ))?;
    if pow_params.nullifier.is_zero() {
        return Err(LinearError::BlockStructure(
            "coinbase nullifier is zero — must be non-zero per consensus rule".into()
        ));
    }

    // Phase 0.5 FeeCollectV1 structural rules (consensus-coinbase.md §3.15):
    //   1. At most one FeeCollectV1 CALL per block (spec says "calls," not
    //      "transactions containing a call" — two 0x06 calls in one tx pass
    //      the old .any() check. Per-call count enforced by flat iteration.)
    //   2. FeeCollectV1 present iff the block's summed FeeV1 fees > 0
    //   3. FeeCollectV1 must be the final transaction
    // FeeV1 layout: [selector 0x00][fee u64 LE][FeeParamsV1]; FeeCollectV1
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

    // Sum FeeV1 fees across the block (checked — overflow is a structural error).
    let mut total_fees: u64 = 0;
    for tx in &block.transactions {
        for c in &tx.contract_calls {
            if !is_native(c) || c.data.first() != Some(&0x00) {
                continue;
            }
            if c.data.len() < 9 {
                return Err(LinearError::BlockStructure(
                    format!("FeeV1 call data too short ({} bytes)", c.data.len())
                ));
            }
            let fee_bytes: [u8; 8] = c.data[1..9].try_into().expect("length checked above");
            total_fees = total_fees.checked_add(u64::from_le_bytes(fee_bytes))
                .ok_or_else(|| LinearError::BlockStructure("FeeV1 fee sum overflow".into()))?;
        }
    }

    match (fee_collect_tx_position, total_fees, fee_collect_call_count) {
        // Present with zero fees — zero-value claim / 0-fee replay (§3.13)
        (Some(_), 0, _) => {
            return Err(LinearError::BlockStructure(
                "FeeCollectV1 present but block has zero FeeV1 fees".into()
            ));
        }
        // Absent with non-zero fees — fees stranded permanently (§3.13)
        (None, f, _) if f > 0 => {
            return Err(LinearError::BlockStructure(
                format!("block pays {} fee units but has no FeeCollectV1 call", f)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A block with correct defaults for empty transactions.
    /// merkle_root for 0 txs is blake3::hash(&[]).
    fn dummy_block() -> Block {
        Block {
            header: super::super::BlockHeader {
                version: 1,
                previous: Blake3Hash::from([0u8; 32]),
                merkle_root: blake3::hash(&[]), // correct for 0 transactions
                timestamp: BlockTimestamp::new(0),
                target: BlockTarget::MAX,
                nonce: 0,
                height: BlockHeight::new(1),
                uncle_merkle_root: [0u8; 32],
                total_reward: BlockReward::ZERO,
                randomx_key: [0u8; 32],
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: PowSource::Native,
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
                assert_eq!(declared, u32::MAX);
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
        ClearInput, Coin, Nullifier as NtNullifier, Output, PoWRewardParamsV1 as PowParams,
    };
    use dwow_sdk::crypto::pasta_prelude::Group;
    use dwow_sdk::crypto::{note::AeadEncryptedNote, BaseBlind, Blind, FuncId, Keypair};
    use dwow_sdk::pasta::pallas;

    /// A structurally valid coinbase transaction: PoWRewardV1 call with
    /// deserializable params and a non-zero nullifier.
    fn coinbase_tx() -> crate::Transaction {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        let coin = Coin::from_attributes(
            &keypair.public,
            1000,
            dwow_native_token_contract::model::DRKW_TOKEN_ID,
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );
        let params = PowParams {
            input: ClearInput {
                value: 1000,
                token_id: dwow_native_token_contract::model::DRKW_TOKEN_ID.inner(),
                value_blind: Blind(pallas::Scalar::zero()),
                token_blind: BaseBlind::ZERO,
                signature_public: keypair.public,
            },
            output: Output {
                value_commit: pallas::Point::identity(),
                token_commit: pallas::Base::zero(),
                coin,
                nullifier: NtNullifier::from_bytes([2u8; 32]).unwrap(),
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
            version: 1,
            contract_calls: vec![crate::ContractCall {
                contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                data,
            }],
            ..Default::default()
        }
    }

    /// A FeeV1 transaction — Phase 0 only reads [selector][fee u64 LE].
    fn fee_tx(fee: u64) -> crate::Transaction {
        let mut data = vec![0x00u8];
        data.extend_from_slice(&fee.to_le_bytes());
        crate::Transaction {
            version: 1,
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
            version: 1,
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
        block.transactions = vec![coinbase_tx(), fee_tx(42_000_000), fee_collect_tx()];
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
        assert!(format!("{:?}", err).contains("zero FeeV1 fees"), "got {:?}", err);
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
    fn phase05_rejects_short_fee_call_data() {
        // Malformed FeeV1 data must be an error, never silently zero (spec §3.12).
        let mut block = dummy_block();
        let mut tx = fee_tx(1);
        tx.contract_calls[0].data = vec![0x00u8, 1, 2]; // < 9 bytes
        block.transactions = vec![coinbase_tx(), tx];
        let err = validate_block_structure(&block).unwrap_err();
        assert!(format!("{:?}", err).contains("too short"), "got {:?}", err);
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
        // contract_id filter: a non-native call starting with 0x00 is NOT a fee.
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
            depth: 1,
            pin_offered: false,
            pin_accepted: false,
            pin_confirmed: 0, // not validated by check_uncles — verify_uncle_split handles this downstream
            header: super::super::BlockHeader {
                version: 1,
                previous: Blake3Hash::from([0u8; 32]),
                merkle_root: blake3::hash(&[]),
                timestamp: BlockTimestamp::new(0),
                target: BlockTarget::MAX,
                nonce,
                height: BlockHeight::new(height),
                uncle_merkle_root: [0u8; 32],
                total_reward: BlockReward::ZERO,
                randomx_key: [0u8; 32],
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: PowSource::Native,
            },
        }
    }

    /// B1: 7 uncles → TooManyUncles (MAX_UNCLE_COUNT = 6)
    #[test]
    fn check_uncles_rejects_too_many() {
        let vm = test_vm();
        let uncles: Vec<UncleBlock> = (0..7).map(|i| dummy_uncle(2, i)).collect();
        let (root, proofs) = build_uncle_merkle(&uncles, &vm);
        let err = check_uncles(
            &uncles, &proofs, &root,
            BlockHeight::new(10), &vm, u32::MAX, &std::collections::HashSet::new(),
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
        let vm = test_vm();
        let uncle = dummy_uncle(8, 42);
        let (root, proofs) = build_uncle_merkle(&[uncle.clone()], &vm);
        // check_uncles uses to_mining_blob() for the canonical key
        let key = *blake3::hash(&uncle.header.to_mining_blob()).as_bytes();
        let mut existing: std::collections::HashSet<[u8; 32]> = std::collections::HashSet::new();
        existing.insert(key);
        let err = check_uncles(
            &[uncle], &proofs, &root,
            BlockHeight::new(10), &vm, u32::MAX, &existing,
        ).unwrap_err();
        match err {
            LinearError::DuplicateUncle(_) => {}
            e => panic!("expected DuplicateUncle, got {:?}", e),
        }
    }

    /// B3: Uncle with impossible PoW (target=0, nonce=0) → UnclePoWInvalid
    #[test]
    fn check_uncles_rejects_invalid_pow() {
        let vm = test_vm();
        let mut uncle = dummy_uncle(8, 0);
        uncle.header.target = BlockTarget::new(0); // impossible to satisfy
        let (root, proofs) = build_uncle_merkle(&[uncle.clone()], &vm);
        let err = check_uncles(
            &[uncle], &proofs, &root,
            BlockHeight::new(10), &vm, 0, &std::collections::HashSet::new(),
        ).unwrap_err();
        match err {
            LinearError::UnclePoWInvalid(_) => {}
            e => panic!("expected UnclePoWInvalid, got {:?}", e),
        }
    }

    /// B4: Fabricated uncle (wrong merkle root) → UncleMerkleRootMismatch.
    /// The merkle root check fires before individual proof verification.
    #[test]
    fn check_uncles_rejects_wrong_merkle_root() {
        let vm = test_vm();
        let uncle_a = dummy_uncle(8, 100);
        let uncle_b = dummy_uncle(8, 200);
        let (_root_a, proofs_a) = build_uncle_merkle(&[uncle_a.clone()], &vm);
        let (root_b, _) = build_uncle_merkle(&[uncle_b], &vm);
        // Use proof from tree A with root from tree B → root mismatch
        let err = check_uncles(
            &[uncle_a], &proofs_a, &root_b,
            BlockHeight::new(10), &vm, u32::MAX, &std::collections::HashSet::new(),
        ).unwrap_err();
        match err {
            LinearError::UncleMerkleRootMismatch(_) => {}
            e => panic!("expected UncleMerkleRootMismatch, got {:?}", e),
        }
    }

    /// B5: Uncle depth > MAX_UNCLE_DEPTH (6) → UncleTooOld
    #[test]
    fn check_uncles_rejects_too_old() {
        let vm = test_vm();
        let uncle = dummy_uncle(2, 42); // uncle at height 2
        let (root, proofs) = build_uncle_merkle(&[uncle.clone()], &vm);
        let current = BlockHeight::new(2 + 6 + 1); // depth = 7 > MAX_UNCLE_DEPTH
        let err = check_uncles(
            &[uncle], &proofs, &root,
            current, &vm, u32::MAX, &std::collections::HashSet::new(),
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
        let vm = test_vm();
        let uncle = dummy_uncle(8, 42);
        let (root, proofs) = build_uncle_merkle(&[uncle.clone()], &vm);
        let result = check_uncles(
            &[uncle], &proofs, &root,
            BlockHeight::new(10), &vm, u32::MAX, &std::collections::HashSet::new(),
        );
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }
}
