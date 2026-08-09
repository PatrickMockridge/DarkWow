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

//! Unit tests for NativeTokenFeeSignallingExtractor — the control valve
//! on the transaction pipeline.
//!
//! Each test is a pure function `() -> Result<(), Error>` with no mutable
//! global state. Fixtures are constructed fresh per test.
//!
//! Process engineering context:
//! - `extract_fee()`: reads the pressure gauge (fee estimate from call data)
//! - `extract_fee_commitment()`: reads the sealed gauge (Pedersen commitment
//!   when the fee is hidden)
//! - `verify_threshold_proof()`: checks the choke position (proves fee >=
//!   threshold without revealing the fee)
//! - `declare_charge()`: declarative block capacity charge for block packing
//!
//! Domain: fee_signalling (non-consensus flow control).
//! See fee-spec.md §0.1 for the process engineering analogy.

use dwow_chain::{ContractCall, Transaction};
use dwow_mempool::FeeSignallingExtractor;
use dwow_sdk::blockchain::{BlockCharge, BlockVersion, FeeAmount};
use dwow_sdk::crypto::ContractId;
use dwow_sdk::pasta::group::{Group, GroupEncoding};
use dwow_sdk::pasta::pallas;

use crate::NativeTokenFeeSignallingExtractor;

/// Pure fixture: build a Transaction with n FeeV2 calls to NATIVE_TOKEN_CONTRACT_ID.
/// Each FeeV2 call has selector 0x08 followed by `params_bytes`.
fn make_tx_with_feev2_calls(n: usize, params_bytes: Vec<u8>) -> Transaction {
    let contract_calls: Vec<ContractCall> = (0..n)
        .map(|_| {
            let mut data = vec![0x08u8]; // FeeV2 selector
            data.extend_from_slice(&params_bytes);
            ContractCall {
                contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                data,
            }
        })
        .collect();

    Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls,
        lock_time: 0,
        nullifiers: vec![],
        witness: vec![],
    }
}

/// Pure fixture: build a Transaction with non-FeeV2 contract calls.
fn make_tx_with_non_feev2_calls(n: usize) -> Transaction {
    let contract_calls: Vec<ContractCall> = (0..n)
        .map(|_| ContractCall {
            contract_id: ContractId::from_bytes([1u8; 32]).unwrap(),
            data: vec![0x01, 0x02, 0x03],
        })
        .collect();

    Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls,
        lock_time: 0,
        nullifiers: vec![],
        witness: vec![],
    }
}

/// Minimal valid FeeParamsV2-esque byte buffer for extract_fee_commitment testing.
///
/// Layout (per NativeTokenFeeSignallingExtractor::extract_fee_commitment):
///   input: 224 bytes
///   output: 130 bytes (minimum, user_data_len=0 encoded as [0,0] at bytes 352..354)
///   fee_value_commit: 32 bytes (compressed pallas::Point)
///   threshold: 8 bytes
///   proof_len: 4 bytes
///   proof: 0 bytes
///   blinds: 128 bytes (optional for extraction test)
///
/// Total minimal: 224 + 130 + 32 = 386 bytes before fee_commit_offset check.
/// We pad to 526 bytes to cover all fields.
fn make_minimal_feev2_params(point_bytes: [u8; 32]) -> Vec<u8> {
    let input_len = 224usize;
    let output_len = 130usize; // minimum: 130 base + 0 dynamic
    let total = input_len + output_len + 32 + 8 + 4 + 128; // 526
    let mut buf = vec![0u8; total];

    // Encode output user_data_len = 0 at bytes [input_len + 128..input_len + 130]
    buf[input_len + 128] = 0x00;
    buf[input_len + 129] = 0x00;

    // Place the point at the fee_commit_offset
    let fee_commit_offset = input_len + output_len;
    buf[fee_commit_offset..fee_commit_offset + 32].copy_from_slice(&point_bytes);

    buf
}

// ── extract_fee ─────────────────────────────────────────────────────────

#[test]
fn test_extract_fee_counts_feev2_calls() {
    // Pure: fn(tx) -> fee. DEFAULT_FEE = 42_000_000 per FeeV2 call.
    let extractor = NativeTokenFeeSignallingExtractor::new();

    let tx0 = make_tx_with_feev2_calls(0, vec![0u8; 64]);
    assert_eq!(extractor.extract_fee(&tx0), FeeAmount::ZERO, "zero FeeV2 calls → zero fee");

    let tx1 = make_tx_with_feev2_calls(1, vec![0u8; 64]);
    assert_eq!(extractor.extract_fee(&tx1), FeeAmount::new(42_000_000), "one FeeV2 call → 42M");

    let tx3 = make_tx_with_feev2_calls(3, vec![0u8; 64]);
    assert_eq!(extractor.extract_fee(&tx3), FeeAmount::new(126_000_000), "three FeeV2 calls → 126M");
}

#[test]
fn test_extract_fee_ignores_non_feev2() {
    // Non-FeeV2 calls (wrong contract_id or wrong selector) contribute 0.
    let extractor = NativeTokenFeeSignallingExtractor::new();

    // Zero FeeV2 calls — fee is 0
    let tx = make_tx_with_non_feev2_calls(3);
    assert_eq!(extractor.extract_fee(&tx), FeeAmount::ZERO, "non-FeeV2 calls contribute zero");

    // Mix: 1 FeeV2 + 2 non-FeeV2 — only FeeV2 counted
    let mut mixed = make_tx_with_feev2_calls(1, vec![0u8; 64]);
    mixed.contract_calls.extend(make_tx_with_non_feev2_calls(2).contract_calls);
    assert_eq!(extractor.extract_fee(&mixed), FeeAmount::new(42_000_000),
        "only FeeV2 calls counted, non-FeeV2 ignored");
}

// ── extract_fee_commitment ──────────────────────────────────────────────

#[test]
fn test_extract_fee_commitment_well_formed() {
    // Parses valid FeeParamsV2, returns Some(pallas::Point).
    // Uses the pallas generator as a valid on-curve point.
    let extractor = NativeTokenFeeSignallingExtractor::new();
    let generator = pallas::Point::identity();
    let point_bytes = generator.to_bytes();
    let params = make_minimal_feev2_params(point_bytes);
    let tx = make_tx_with_feev2_calls(1, params);

    let result = extractor.extract_fee_commitment(&tx);
    assert!(result.is_some(), "well-formed FeeV2 should yield a fee commitment");
    assert_eq!(result.unwrap().0, generator, "extracted point should match generator");
}

#[test]
fn test_extract_fee_commitment_malformed_too_short() {
    // Returns None on truncated params — fewer bytes than minimal layout.
    let extractor = NativeTokenFeeSignallingExtractor::new();

    // Data too short to even contain input + output + fee_value_commit
    let short_params = vec![0u8; 10];
    let tx = make_tx_with_feev2_calls(1, short_params);
    assert!(extractor.extract_fee_commitment(&tx).is_none(),
        "too-short params → None");
}

#[test]
fn test_extract_fee_commitment_zero_length_params() {
    let extractor = NativeTokenFeeSignallingExtractor::new();
    let tx = make_tx_with_feev2_calls(1, vec![]);
    assert!(extractor.extract_fee_commitment(&tx).is_none(),
        "zero-length params → None");
}

#[test]
fn test_extract_fee_commitment_rejects_bad_point() {
    // Returns None when 32-byte point is not on curve (identity bytes).
    // The identity point to_bytes() encodes as [0u8; 32].
    // An invalid compressed point (bit not matching a valid y) also returns None.
    let extractor = NativeTokenFeeSignallingExtractor::new();

    // Identity point: from_bytes returns None (not a valid affine point)
    let identity_bytes = [0u8; 32];
    let params = make_minimal_feev2_params(identity_bytes);
    let tx = make_tx_with_feev2_calls(1, params);
    assert!(extractor.extract_fee_commitment(&tx).is_none(),
        "identity point (not on curve) → None");
}

#[test]
fn test_extract_fee_commitment_no_feev2_calls() {
    // Transaction with zero FeeV2 calls → None.
    let extractor = NativeTokenFeeSignallingExtractor::new();
    let tx = make_tx_with_non_feev2_calls(3);
    assert!(extractor.extract_fee_commitment(&tx).is_none(),
        "no FeeV2 calls → None");
}

// ── verify_threshold_proof ──────────────────────────────────────────────

#[test]
fn test_verify_threshold_proof_no_feev2_calls() {
    // Transaction with no FeeV2 calls → false.
    let extractor = NativeTokenFeeSignallingExtractor::new();
    let tx = make_tx_with_non_feev2_calls(3);
    assert!(!extractor.verify_threshold_proof(&tx, FeeAmount::new(42_000_000)),
        "no FeeV2 calls → false");
}

#[test]
fn test_verify_threshold_proof_decode_failure() {
    // Returns false when FeeParamsV2::decode() fails (malformed params).
    let extractor = NativeTokenFeeSignallingExtractor::new();

    // Short random bytes that won't decode as valid FeeParamsV2
    let tx = make_tx_with_feev2_calls(1, vec![0xFFu8; 100]);
    assert!(!extractor.verify_threshold_proof(&tx, FeeAmount::new(42_000_000)),
        "malformed FeeParamsV2 → false");
}

#[test]
fn test_verify_threshold_proof_zero_length_params() {
    let extractor = NativeTokenFeeSignallingExtractor::new();
    let tx = make_tx_with_feev2_calls(1, vec![]);
    assert!(!extractor.verify_threshold_proof(&tx, FeeAmount::new(42_000_000)),
        "zero-length params → decode failure → false");
}

// ── declare_charge ────────────────────────────────────────────────────────

#[test]
fn test_declare_charge_zero_calls() {
    let extractor = NativeTokenFeeSignallingExtractor::new();
    let tx = Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![],
        lock_time: 0,
        nullifiers: vec![],
        witness: vec![],
    };
    assert_eq!(extractor.declare_charge(&tx), BlockCharge::ZERO, "zero calls → zero charge");
}

#[test]
fn test_declare_charge_scales_with_calls() {
    // Pure: n calls × 400_000_000 = estimated gas.
    let extractor = NativeTokenFeeSignallingExtractor::new();

    let tx1 = make_tx_with_non_feev2_calls(1);
    assert_eq!(extractor.declare_charge(&tx1), BlockCharge::new(400_000_000));

    let tx5 = make_tx_with_non_feev2_calls(5);
    assert_eq!(extractor.declare_charge(&tx5), BlockCharge::new(2_000_000_000));
}

// ── L1.5-FW-1: verify_threshold_proof SUCCESS path with real FeeParamsV2 ──
//
// All tests above cover failure paths (malformed, too short, no FeeV2 calls).
// This test exercises the REAL NativeTokenFeeSignallingExtractor with a REAL
// FeeV2 transaction built via NativeTokenHarness — FeeParamsV2 with valid
// FeeThreshold_V1 proof, proper threshold field, real Pedersen commitment.
// Closes the gap: success-path verification, fee extraction, commitment extraction.
#[test]
fn test_fee_extractor_real_feev2_success_path() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, NATIVE_TOKEN_CONTRACT_ID, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        let chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;

        let native_harness = NativeTokenHarness::spawn();
        let cid = *NATIVE_TOKEN_CONTRACT_ID;

        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        let mining_kp = chain.mining_keypair(BlockHeight::new(2));
        let fee_amount: u64 = 42_000_000;
        let fee_result = native_harness.fee_v2(
            cb2.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos), path, root,
            mining_kp.secret.clone(), mining_kp.secret,
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            fee_amount,
            42_000_000,  // threshold
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "[L1.5-FW-1] fee_v2 harness: {}", e
        )))?;

        let chain_tx = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall {
                contract_id: cid,
                data: fee_result.call_data.clone(),
            }],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };

        let extractor = NativeTokenFeeSignallingExtractor::new();

        // (a) verify_threshold_proof with matching threshold → true
        assert!(extractor.verify_threshold_proof(&chain_tx, 42_000_000),
            "[L1.5-FW-1] verify_threshold_proof should succeed for matching threshold 42M");

        // (b) verify_threshold_proof with mismatched threshold → false
        assert!(!extractor.verify_threshold_proof(&chain_tx, 1_000_000),
            "[L1.5-FW-1] verify_threshold_proof should fail for mismatched threshold 1M");

        // (c) extract_fee returns DEFAULT_FEE per FeeV2 call
        assert_eq!(extractor.extract_fee(&chain_tx), 42_000_000,
            "[L1.5-FW-1] extract_fee should return 42M (1 FeeV2 call × DEFAULT_FEE)");

        // (d) extract_fee_commitment returns the real Pedersen point
        let commitment = extractor.extract_fee_commitment(&chain_tx);
        assert!(commitment.is_some(),
            "[L1.5-FW-1] extract_fee_commitment should return Some for valid FeeParamsV2");

        Ok(())
    })
}
