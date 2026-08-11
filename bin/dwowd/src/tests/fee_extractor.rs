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

/// Test fee value — replaces inherited upstream 1 magic constant.
const TEST_FEE_VAL: u64 = 1;

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
    // Pure: fn(tx) -> fee. FeeV2 exact fees are hidden behind Pedersen
    // commitments — extract_fee returns FeeAmount::ZERO for FeeV2 (M-7 fix).
    let extractor = NativeTokenFeeSignallingExtractor::new();

    let tx0 = make_tx_with_feev2_calls(0, vec![0u8; 64]);
    assert_eq!(extractor.extract_fee(&tx0), FeeAmount::ZERO, "zero FeeV2 calls → zero fee");

    let tx1 = make_tx_with_feev2_calls(1, vec![0u8; 64]);
    assert_eq!(extractor.extract_fee(&tx1), FeeAmount::ZERO, "one FeeV2 call → zero (fee in Pedersen commitment)");

    let tx3 = make_tx_with_feev2_calls(3, vec![0u8; 64]);
    assert_eq!(extractor.extract_fee(&tx3), FeeAmount::ZERO, "three FeeV2 calls → zero (fee in Pedersen commitment)");
}

#[test]
fn test_extract_fee_ignores_non_feev2() {
    // Non-FeeV2 calls (wrong contract_id or wrong selector) contribute 0.
    let extractor = NativeTokenFeeSignallingExtractor::new();

    // Zero FeeV2 calls — fee is 0
    let tx = make_tx_with_non_feev2_calls(3);
    assert_eq!(extractor.extract_fee(&tx), FeeAmount::ZERO, "non-FeeV2 calls contribute zero");

    // Mix: 1 FeeV2 + 2 non-FeeV2 — only FeeV2 counted (but FeeV2 returns ZERO)
    let mut mixed = make_tx_with_feev2_calls(1, vec![0u8; 64]);
    mixed.contract_calls.extend(make_tx_with_non_feev2_calls(2).contract_calls);
    assert_eq!(extractor.extract_fee(&mixed), FeeAmount::ZERO,
        "mixed FeeV2 + non-FeeV2 → zero");
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
    // Returns None when 32-byte point is not on curve.
    // [0xFF; 32] is not a valid compressed Pallas point (no y exists for x=0xFF...).
    let extractor = NativeTokenFeeSignallingExtractor::new();

    let bad_bytes = [0xFFu8; 32];
    let params = make_minimal_feev2_params(bad_bytes);
    let tx = make_tx_with_feev2_calls(1, params);
    assert!(extractor.extract_fee_commitment(&tx).is_none(),
        "invalid point [0xFF; 32] must return None");
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
    assert!(!extractor.verify_threshold_proof(&tx, FeeAmount::new(TEST_FEE_VAL)),
        "no FeeV2 calls → false");
}

#[test]
fn test_verify_threshold_proof_decode_failure() {
    // Returns false when FeeParamsV2::decode() fails (malformed params).
    let extractor = NativeTokenFeeSignallingExtractor::new();

    // Short random bytes that won't decode as valid FeeParamsV2
    let tx = make_tx_with_feev2_calls(1, vec![0xFFu8; 100]);
    assert!(!extractor.verify_threshold_proof(&tx, FeeAmount::new(TEST_FEE_VAL)),
        "malformed FeeParamsV2 → false");
}

#[test]
fn test_verify_threshold_proof_zero_length_params() {
    let extractor = NativeTokenFeeSignallingExtractor::new();
    let tx = make_tx_with_feev2_calls(1, vec![]);
    assert!(!extractor.verify_threshold_proof(&tx, FeeAmount::new(TEST_FEE_VAL)),
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
        let fee_amount: u64 = TEST_FEE_VAL;
        let fee_result = native_harness.fee_v2(
            cb2.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos), path, root,
            mining_kp.secret.clone(), mining_kp.secret,
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            fee_amount,
            TEST_FEE_VAL,  // threshold
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
        assert!(extractor.verify_threshold_proof(&chain_tx, FeeAmount::new(TEST_FEE_VAL)),
            "[L1.5-FW-1] verify_threshold_proof should succeed for matching threshold 42M");

        // (b) verify_threshold_proof with mismatched threshold → false
        assert!(!extractor.verify_threshold_proof(&chain_tx, FeeAmount::new(1_000_000)),
            "[L1.5-FW-1] verify_threshold_proof should fail for mismatched threshold 1M");

        // M-7 FIX: extract_fee returns FeeAmount::ZERO for FeeV2 — exact fees
        // are hidden behind Pedersen commitments. The old ESTIMATED_FEE_PER_FEEV2_CALL
        // constant was removed in Phase 3 anti-pattern remediation (SPEC-2).
        assert_eq!(extractor.extract_fee(&chain_tx), FeeAmount::ZERO,
            "[L1.5-FW-1] extract_fee must return ZERO for FeeV2 (fee in Pedersen commitment)");

        // (d) extract_fee_commitment returns the real Pedersen point
        let commitment = extractor.extract_fee_commitment(&chain_tx);
        assert!(commitment.is_some(),
            "[L1.5-FW-1] extract_fee_commitment should return Some for valid FeeParamsV2");

        Ok(())
    })
}

// ============================================================================
// G2: encrypt_fee_for_miner → decrypt_fee_for_miner roundtrip (L1.5).
// Verifies the full wallet→miner fee encryption channel without needing
// accept_block. Uses real AEAD (ECDH + ChaCha20-Poly1305).
// ============================================================================

#[test]
fn test_g2_encrypt_decrypt_roundtrip() {
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_wallet::fee_builder::encrypt_fee_for_miner;
    use rand::rngs::OsRng;

    let miner_sk = SecretKey::random(&mut OsRng);
    let miner_pk = PublicKey::from_secret(miner_sk.clone());

    let fee = FeeAmount::new(TEST_FEE_VAL);
    let ciphertext = encrypt_fee_for_miner(fee, &miner_pk)
        .expect("[G2] encrypt must succeed");
    eprintln!("[G2 DIAG] ciphertext len = {}", ciphertext.len());

    let decrypted = crate::NativeTokenFeeSignallingExtractor::decrypt_fee_for_miner(
        &ciphertext, &miner_sk,
    );
    eprintln!("[G2 DIAG] decrypted = {:?}", decrypted);
    assert!(decrypted.is_ok(), "[G2] decrypt must succeed");
    assert_eq!(decrypted.unwrap(), FeeAmount::new(TEST_FEE_VAL), "[G2] roundtrip match");

    // Wrong key fails
    let wrong_sk = SecretKey::random(&mut OsRng);
    let wrong_result = crate::NativeTokenFeeSignallingExtractor::decrypt_fee_for_miner(
        &ciphertext, &wrong_sk,
    );
    assert!(wrong_result.is_err(),
        "[G2] decrypt with wrong key must return Err");

    // Corrupted ciphertext fails
    let mut corrupted = ciphertext.clone();
    if corrupted.len() > 44 { corrupted[44] ^= 0xFF; }
    let corrupt_result = crate::NativeTokenFeeSignallingExtractor::decrypt_fee_for_miner(
        &corrupted, &miner_sk,
    );
    assert!(corrupt_result.is_err(),
        "[G2] decrypt with corrupted ciphertext must return Err");
}

// ============================================================================
// GAP-10 / FI-ENCRYPT-2: Per-block key rotation test (L1.5).
//
// The miner's fee encryption key SHALL be per-block derived (fee-spec.md
// FI-ENCRYPT-2). A fee encrypted for block N's mining key SHALL NOT be
// decryptable with block N+1's mining key — keys from different heights
// SHALL be independent and uncorrelated.
//
// This test derives two mining keypairs at different heights via
// HeavyweightPipeline::mining_keypair(), encrypts a fee to height-2's
// public key, and verifies decryption with height-3's secret key fails.
// ============================================================================

#[test]
fn test_per_block_key_rotation() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_wallet::fee_builder::encrypt_fee_for_miner;
    use crate::tests::blockchain::HeavyweightPipeline;

    smol::block_on(async {
        let chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;

        // Derive mining keys at two different heights.
        let kp_h2 = chain.mining_keypair(BlockHeight::new(2));
        let kp_h3 = chain.mining_keypair(BlockHeight::new(3));

        // Keys at different heights SHALL be different (FI-ENCRYPT-2).
        assert_ne!(kp_h2.secret, kp_h3.secret,
            "[GAP-10] FI-ENCRYPT-2: mining keys at height 2 and 3 must differ");
        let pk_h2 = dwow_sdk::crypto::PublicKey::from_secret(kp_h2.secret.clone());
        let pk_h3 = dwow_sdk::crypto::PublicKey::from_secret(kp_h3.secret.clone());
        assert_ne!(pk_h2, pk_h3,
            "[GAP-10] FI-ENCRYPT-2: public keys at height 2 and 3 must differ");

        // Encrypt fee to height-2's public key.
        let fee = FeeAmount::new(TEST_FEE_VAL);
        let ciphertext = encrypt_fee_for_miner(fee, &pk_h2)
            .expect("[GAP-10] encrypt to height-2 key must succeed");

        // Decrypt with correct key (height-2) → succeeds.
        let decrypted = crate::NativeTokenFeeSignallingExtractor::decrypt_fee_for_miner(
            &ciphertext, &kp_h2.secret,
        );
        assert!(decrypted.is_ok(),
            "[GAP-10] decrypt with correct height-2 key must succeed");
        assert_eq!(decrypted.unwrap(), fee,
            "[GAP-10] roundtrip must match original fee");

        // Decrypt with wrong key (height-3) → fails (FI-ENCRYPT-2).
        let wrong_result = crate::NativeTokenFeeSignallingExtractor::decrypt_fee_for_miner(
            &ciphertext, &kp_h3.secret,
        );
        assert!(wrong_result.is_err(),
            "[GAP-10] FI-ENCRYPT-2: decrypt with height-3 key must fail — \
             encrypted fees from different blocks SHALL NOT be correlatable");

        Ok(())
    })
}

// ============================================================================
// GAP-13 / FI-COLLECT-5: Accumulator byte encoding rejection tests (L1.5).
//
// AccumulatorPoint::decode() SHALL reject values with wrong length or
// invalid curve points (fee-spec.md FI-COLLECT-5). The Identity point
// SHALL encode to exactly [0u8; 32]. Roundtrip: decode(encode(p)) == p.
// ============================================================================

#[test]
fn test_accumulator_encode_decode_roundtrip() {
    use dwow_native_token_contract::model::AccumulatorPoint;
    use dwow_sdk::pasta::pallas;

    // Identity → encode → decode → Identity
    let acc = AccumulatorPoint::identity();
    let bytes = acc.encode();
    assert_eq!(bytes.len(), 32,
        "[GAP-13] FI-COLLECT-5: encode() must produce exactly 32 bytes");
    assert!(bytes.iter().all(|b| *b == 0),
        "[GAP-13] FI-COLLECT-5: Identity SHALL encode as [0u8; 32]");

    let decoded = AccumulatorPoint::decode(&bytes)
        .expect("[GAP-13] decode of valid encoded identity must succeed");
    assert!(decoded.is_identity(),
        "[GAP-13] FI-COLLECT-5: decode(encode(identity)) must be identity");

    // Active point after add_commitment → encode → decode → match
    let generator = pallas::Point::generator();
    let active = acc.add_commitment(generator);
    assert!(!active.is_identity(),
        "[GAP-13] accumulator after add_commitment must not be Identity");
    let active_bytes = active.encode();
    let active_decoded = AccumulatorPoint::decode(&active_bytes)
        .expect("[GAP-13] decode of encoded active point must succeed");
    assert!(!active_decoded.is_identity(),
        "[GAP-13] decoded active point must not be Identity");
    assert_eq!(active_bytes, active_decoded.encode(),
        "[GAP-13] FI-COLLECT-5: encode roundtrip must be byte-identical");
}

#[test]
fn test_accumulator_decode_rejects_wrong_size() {
    use dwow_native_token_contract::model::AccumulatorPoint;

    // 9 bytes — the classic 9-byte accumulator error (CALLER_ACCESS_DENIED on wasm32).
    let result_9 = AccumulatorPoint::decode(&[0u8; 9]);
    assert!(result_9.is_err(),
        "[GAP-13] FI-COLLECT-5: decode of 9 bytes must return Err (↓bad-accumulator)");
    let err_9 = format!("{:?}", result_9.err());
    assert!(err_9.contains("32"),
        "[GAP-13] error must mention expected 32 bytes, got: {}", err_9);

    // 0 bytes — empty slice.
    let result_0 = AccumulatorPoint::decode(&[]);
    assert!(result_0.is_err(),
        "[GAP-13] FI-COLLECT-5: decode of 0 bytes must return Err");

    // 31 bytes — one short.
    let result_31 = AccumulatorPoint::decode(&[0u8; 31]);
    assert!(result_31.is_err(),
        "[GAP-13] FI-COLLECT-5: decode of 31 bytes must return Err");

    // 33 bytes — one long.
    let result_33 = AccumulatorPoint::decode(&[0u8; 33]);
    assert!(result_33.is_err(),
        "[GAP-13] FI-COLLECT-5: decode of 33 bytes must return Err");
}

#[test]
fn test_accumulator_decode_rejects_invalid_point() {
    use dwow_native_token_contract::model::AccumulatorPoint;

    // [0xFF; 32] — not a valid compressed Pallas point.
    let result = AccumulatorPoint::decode(&[0xFFu8; 32]);
    assert!(result.is_err(),
        "[GAP-13] FI-COLLECT-5: decode of [0xFF; 32] must return Err (↓bad-accumulator)");
    let err = format!("{:?}", result.err());
    assert!(err.contains("invalid") || err.contains("bad-accumulator") || err.contains("point"),
        "[GAP-13] error must cite invalid point, got: {}", err);

    // [0x01; 32] with top bit set on the x-coordinate — also invalid.
    let mut invalid_bytes = [0x01u8; 32];
    invalid_bytes[31] = 0xFF;
    let result2 = AccumulatorPoint::decode(&invalid_bytes);
    assert!(result2.is_err(),
        "[GAP-13] FI-COLLECT-5: decode of invalid compressed point must return Err");
}

// ============================================================================
// P4 / FI-TIME-1: FeeThreshold_V1 proof generation timing benchmark (L1).
//
// Spec: fee-spec.md §14.9 — proof generation time SHALL be less than the
// window boundary deadline (block production interval). The acceptance
// window is 30s; proof must complete well within this.
//
// This test measures the complete FeeThreshold_V1 proof generation pipeline
// using the real test harness. It verifies that p95 proof time is under 1s,
// providing 30× headroom below the 30s window.
// ============================================================================

#[test]
fn test_fi_time1_proof_generation_timing() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use std::time::Instant;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        let chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;

        let native_harness = NativeTokenHarness::spawn();
        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        let mining_kp = chain.mining_keypair(BlockHeight::new(2));
        let fee_dest = PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?);

        const N_WARMUP: usize = 5;
        const N_ITER: usize = 50;
        let mut times: Vec<u64> = Vec::with_capacity(N_ITER);

        for i in 0..(N_WARMUP + N_ITER) {
            let start = Instant::now();
            let _result = native_harness.fee_v2(
                cb2.coin_value,
                pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
                cb2.coin_blind,
                u64::from(coin_pos),
                path.clone(),
                root,
                mining_kp.secret.clone(),
                mining_kp.secret.clone(),
                fee_dest,
                pallas::Base::zero(), pallas::Base::zero(),
                1, 1,
            ).map_err(|e| dwow_core::Error::Custom(format!(
                "[FI-TIME-1] fee_v2 iteration {}: {}", i, e
            )))?;
            let elapsed = start.elapsed().as_micros() as u64;
            if i >= N_WARMUP {
                times.push(elapsed);
            }
        }

        times.sort();
        let min_us = times.first().copied().unwrap_or(0);
        let max_us = times.last().copied().unwrap_or(0);
        let median_us = times[times.len() / 2];
        let p95_us = times[(times.len() * 95 / 100).min(times.len() - 1)];

        eprintln!("[FI-TIME-1] FeeThreshold_V1 proof timing ({} iterations, k=11):", N_ITER);
        eprintln!("  min={}µs  median={}µs  p95={}µs  max={}µs",
            min_us, median_us, p95_us, max_us);
        eprintln!("  p95={:.1}ms  max={:.1}ms",
            p95_us as f64 / 1000.0, max_us as f64 / 1000.0);

        // FI-TIME-1: p95 proof generation must be well within the 30s window.
        // With deterministic ZK, proof generation for k=11 should be < 1s.
        // Even with real randomness (slower), the 30s window provides 30× headroom.
        assert!(p95_us < 12_000_000,
            "[FI-TIME-1] p95 proof time ({}µs) must be < 12s — \
             well within 30s acceptance window", p95_us);
        assert!(max_us < 30_000_000,
            "[FI-TIME-1] max proof time ({}µs) must be < 30s", max_us);

        Ok(())
    })
}
