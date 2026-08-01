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

//! Bridge contract integration tests
//!
//! These tests verify the bridge contract's:
//! - Data structure encoding/decoding
//! - Model type invariants
//! - Constants
//!
//! ZK proofs and on-chain execution are tested by the heavyweight pipeline
//! (`test_heavyweight_bridge` and `test_relayer_lifecycle_heavyweight`).

use dwow_bridge_contract::{
    model::{DepositParams, ExternalChain, ExternalChainProof, UpdateConfigParams, WithdrawParams},
    BRIDGE_CONTRACT_AZT_CONFIRMATIONS, BRIDGE_CONTRACT_XMR_CONFIRMATIONS,
    BRIDGE_CONTRACT_ZEC_CONFIRMATIONS,
};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::crypto::{IntentCommitment, IntentNullifier, PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;

/// Helper to create an IntentCommitment from bytes
fn make_commitment(seed: u64) -> IntentCommitment {
    let base = pallas::Base::from(seed);
    IntentCommitment::from_base(base)
}

/// Helper to create an IntentNullifier from bytes
fn make_nullifier(seed: u64) -> IntentNullifier {
    let base = pallas::Base::from(seed);
    IntentNullifier::from_base(base)
}

#[test]
fn test_external_chain_encoding() {
    // Verify all variants roundtrip through serialize/deserialize
    for chain in &[ExternalChain::Ethereum, ExternalChain::Monero, ExternalChain::Zcash,
                   ExternalChain::Aztec, ExternalChain::Litecoin] {
        let encoded = serialize(chain);
        let decoded: ExternalChain = deserialize(&encoded).unwrap();
        let re_encoded = serialize(&decoded);
        assert_eq!(encoded, re_encoded, "ExternalChain variant mismatch");
    }
}

#[test]
fn test_deposit_params_encoding() {
    let params = DepositParams {
        commitment: make_commitment(1),
        recipient_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(1))),
        bridge_nonce: 5,
        chain: ExternalChain::Monero,
        external_block_hash: [3u8; 32],
        merkle_proof: vec![[4u8; 32], [5u8; 32], [6u8; 32]],
        external_state_root: [7u8; 32],
        fee: 100,
        proof: vec![0xAA, 0xBB, 0xCC],
        chain_proof: ExternalChainProof::Ethereum,
    };

    let encoded = serialize(&params);
    let decoded: DepositParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.commitment, params.commitment);
    assert_eq!(decoded.recipient_pub, params.recipient_pub);
    assert_eq!(decoded.bridge_nonce, 5);
    assert_eq!(decoded.external_block_hash, [3u8; 32]);
    assert_eq!(decoded.merkle_proof.len(), 3);
    assert_eq!(decoded.external_state_root, [7u8; 32]);
    assert_eq!(decoded.fee, 100);
    assert_eq!(decoded.proof, vec![0xAA, 0xBB, 0xCC]);
    assert!(matches!(decoded.chain_proof, ExternalChainProof::Ethereum));
}

#[test]
fn test_deposit_params_empty_merkle_proof() {
    // Verify serialization handles empty merkle proof
    let params = DepositParams {
        commitment: make_commitment(1),
        recipient_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(1))),
        bridge_nonce: 0,
        chain: ExternalChain::Ethereum,
        external_block_hash: [0u8; 32],
        merkle_proof: vec![],
        external_state_root: [0u8; 32],
        fee: 0,
        proof: vec![],
        chain_proof: ExternalChainProof::Ethereum,
    };

    let encoded = serialize(&params);
    let decoded: DepositParams = deserialize(&encoded).unwrap();

    assert!(decoded.merkle_proof.is_empty());
    assert!(decoded.proof.is_empty());
    assert_eq!(decoded.fee, 0);
    assert!(matches!(decoded.chain_proof, ExternalChainProof::Ethereum));
}

#[test]
fn test_withdraw_params_encoding() {
    let params = WithdrawParams {
        nullifier: make_nullifier(100),
        recipient_hash: [10u8; 32],
        deposit_leaf: pallas::Base::from(200u64),
        amount: 5000,
        proof: vec![0x11, 0x22, 0x33, 0x44],
        fee: 50,
        timeout_height: 1000,
        feed_mode: 1,
        max_fee_bp: Some(500),
        expected_root: pallas::Base::zero(),
    };

    let encoded = serialize(&params);
    let decoded: WithdrawParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.nullifier, make_nullifier(100));
    assert_eq!(decoded.recipient_hash, [10u8; 32]);
    assert_eq!(decoded.amount, 5000);
    assert_eq!(decoded.proof, vec![0x11, 0x22, 0x33, 0x44]);
    assert_eq!(decoded.fee, 50);
    assert_eq!(decoded.timeout_height, 1000);
    assert_eq!(decoded.feed_mode, 1);
    assert_eq!(decoded.max_fee_bp, Some(500));
}

#[test]
fn test_withdraw_params_optional_fields() {
    // Verify None values for optional fields roundtrip correctly
    let params = WithdrawParams {
        nullifier: make_nullifier(1),
        recipient_hash: [0u8; 32],
        deposit_leaf: pallas::Base::zero(),
        amount: 0,
        proof: vec![],
        fee: 0,
        timeout_height: 0,
        feed_mode: 0,
        max_fee_bp: None,
        expected_root: pallas::Base::zero(),
    };

    let encoded = serialize(&params);
    let decoded: WithdrawParams = deserialize(&encoded).unwrap();

    assert!(decoded.max_fee_bp.is_none());
    assert_eq!(decoded.feed_mode, 0);
    assert_eq!(decoded.timeout_height, 0);
}

#[test]
fn test_update_config_params_encoding() {
    let params = UpdateConfigParams {
        deposit_fee: 100,
        withdrawal_fee: 200,
        min_confirmations: 10,
        max_deposit: 1_000_000_000,
        max_withdrawal: 500_000_000,
        gov_pub_x: pallas::Base::zero(),
        gov_pub_y: pallas::Base::zero(),
        config_nullifier: pallas::Base::zero(),
    };

    let encoded = serialize(&params);
    let decoded: UpdateConfigParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.deposit_fee, 100);
    assert_eq!(decoded.withdrawal_fee, 200);
    assert_eq!(decoded.min_confirmations, 10);
    assert_eq!(decoded.max_deposit, 1_000_000_000);
    assert_eq!(decoded.max_withdrawal, 500_000_000);
}

#[test]
fn test_update_config_params_max_values() {
    let params = UpdateConfigParams {
        deposit_fee: u64::MAX,
        withdrawal_fee: u64::MAX,
        min_confirmations: u32::MAX,
        max_deposit: u64::MAX,
        max_withdrawal: u64::MAX,
        gov_pub_x: pallas::Base::zero(),
        gov_pub_y: pallas::Base::zero(),
        config_nullifier: pallas::Base::zero(),
    };

    let encoded = serialize(&params);
    let decoded: UpdateConfigParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.deposit_fee, u64::MAX);
    assert_eq!(decoded.withdrawal_fee, u64::MAX);
    assert_eq!(decoded.min_confirmations, u32::MAX);
    assert_eq!(decoded.max_deposit, u64::MAX);
    assert_eq!(decoded.max_withdrawal, u64::MAX);
}

#[test]
fn test_bridge_constants() {
    // External chain confirmations
    assert_eq!(BRIDGE_CONTRACT_XMR_CONFIRMATIONS, 10);
    assert_eq!(BRIDGE_CONTRACT_ZEC_CONFIRMATIONS, 10);
    assert_eq!(BRIDGE_CONTRACT_AZT_CONFIRMATIONS, 5);
}

#[test]
fn test_external_chain_discrimination() {
    // Different chain variants must serialize differently
    let eth = serialize(&ExternalChain::Ethereum);
    let xmr = serialize(&ExternalChain::Monero);
    let zec = serialize(&ExternalChain::Zcash);

    assert_ne!(eth, xmr);
    assert_ne!(xmr, zec);
    assert_ne!(eth, zec);
}
