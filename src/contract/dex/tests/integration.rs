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

//! DEX contract integration tests

use dwow_dex_contract::{
    model::{
        AcceptSwapParams, CancelSwapParams, CreateSwapParams, ExecuteSwapParams, InitializeParams,
        TransparencyConfig, UpdateConfigParams,
    },
    DexFunction, DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_INFO_TREE, DEX_CONTRACT_PARTICIPANTS_TREE,
    DEX_CONTRACT_SWAPS_TREE,
};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::{
    crypto::{schnorr::Signature, Nullifier, PublicKey, SecretKey},
    pasta::pallas,
};
use dwow_promissory_note_contract::model::CapCommitment;

/// Helper to create PublicKey from a numeric seed
fn make_pubkey(seed: u64) -> PublicKey {
    let secret = SecretKey::from_base(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

/// Helper to create [u8; 32] from a numeric seed
fn make_bytes32(seed: u64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    let seed_bytes = seed.to_le_bytes();
    bytes[..seed_bytes.len()].copy_from_slice(&seed_bytes);
    bytes
}

#[test]
fn test_dex_function_enum_valid() {
    assert!(DexFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(DexFunction::try_from(0x01).is_ok()); // CreateSwapV1
    assert!(DexFunction::try_from(0x02).is_ok()); // AcceptSwapV1
    assert!(DexFunction::try_from(0x03).is_ok()); // ExecuteSwapV1
    assert!(DexFunction::try_from(0x04).is_ok()); // CancelSwapV1
    assert!(DexFunction::try_from(0x05).is_ok()); // UpdateConfigV1
    assert!(DexFunction::try_from(0x06).is_ok()); // SetTransparencyLevelV1
    assert!(DexFunction::try_from(0x07).is_ok()); // ExecuteSwapFeeV1
    assert!(DexFunction::try_from(0x08).is_ok()); // ExecuteSwapSlippageV1
}

#[test]
fn test_dex_function_enum_invalid() {
    assert!(DexFunction::try_from(0xFF).is_err());
    assert!(DexFunction::try_from(0x10).is_err());
    assert!(DexFunction::try_from(0x09).is_err());
}

#[test]
fn test_constants() {
    assert_eq!(DEX_CONTRACT_SWAPS_TREE, "swaps");
    assert_eq!(DEX_CONTRACT_PARTICIPANTS_TREE, "participants");
    assert_eq!(DEX_CONTRACT_CONFIG_TREE, "config");
    assert_eq!(DEX_CONTRACT_INFO_TREE, "info");
}

#[test]
fn test_initialize_params_encoding() {
    let params = InitializeParams {
        timeout: 100,
        fee: 2500,
        transparency_config: TransparencyConfig::default(),
    };

    let encoded = serialize(&params);
    let decoded: InitializeParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.timeout, params.timeout);
    assert_eq!(decoded.fee, params.fee);
}

#[test]
fn test_create_swap_params_encoding() {
    let params = CreateSwapParams {
        swap_id: make_bytes32(1),
        offer_token: make_bytes32(2),
        offer_amount: 1000,
        request_token: make_bytes32(3),
        request_amount: 500,
        lock_commitment: CapCommitment::decode(&[0u8; 32]).unwrap(),
        nullifier: Nullifier::from_bytes(make_bytes32(1)).unwrap(),
        signature_public: make_pubkey(1),
        fee: 100,
        open_execution: false,
        tx_binding: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };

    let encoded = serialize(&params);
    let decoded: CreateSwapParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, params.swap_id);
    assert_eq!(decoded.offer_amount, params.offer_amount);
    assert_eq!(decoded.request_amount, params.request_amount);
    assert_eq!(decoded.fee, params.fee);
    assert_eq!(decoded.open_execution, params.open_execution);
}

#[test]
fn test_accept_swap_params_encoding() {
    let params = AcceptSwapParams {
        swap_id: make_bytes32(1),
        lock_commitment: CapCommitment::decode(&[0u8; 32]).unwrap(),
        nullifier: Nullifier::from_bytes(make_bytes32(1)).unwrap(),
        signature_public: make_pubkey(2),
        fee: 50,
        immediate_execute: false,
        tx_binding: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };

    let encoded = serialize(&params);
    let decoded: AcceptSwapParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, params.swap_id);
    assert_eq!(decoded.fee, params.fee);
    assert_eq!(decoded.immediate_execute, params.immediate_execute);
}

#[test]
fn test_execute_swap_params_encoding() {
    let params = ExecuteSwapParams {
        swap_id: make_bytes32(1),
        alice_secret: make_bytes32(2),
        bob_secret: make_bytes32(3),
        alice_lock: CapCommitment::decode(&[0u8; 32]).unwrap(),
        bob_lock: CapCommitment::decode(&[0u8; 32]).unwrap(),
        alice_nullifier: Nullifier::from_bytes(make_bytes32(1)).unwrap(),
        bob_nullifier: Nullifier::from_bytes(make_bytes32(1)).unwrap(),
        proof: vec![1, 2, 3],
        fee: 25,
        tx_binding: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };

    let encoded = serialize(&params);
    let decoded: ExecuteSwapParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, params.swap_id);
    assert_eq!(decoded.fee, params.fee);
}

#[test]
fn test_cancel_swap_params_encoding() {
    let params = CancelSwapParams {
        swap_id: make_bytes32(1),
        secret: make_bytes32(2),
        nullifier: Nullifier::from_bytes(make_bytes32(1)).unwrap(),
        proof: vec![4, 5, 6],
        fee: 10,
        tx_binding: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };

    let encoded = serialize(&params);
    let decoded: CancelSwapParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, params.swap_id);
    assert_eq!(decoded.fee, params.fee);
}

#[test]
fn test_update_config_params_encoding() {
    let params = UpdateConfigParams {
        timeout: 200,
        fee: 3000,
        gov_pub_x: pallas::Base::zero(),
        gov_pub_y: pallas::Base::zero(),
        gov_nullifier: pallas::Base::zero(),
        tx_binding: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };

    let encoded = serialize(&params);
    let decoded: UpdateConfigParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.timeout, params.timeout);
    assert_eq!(decoded.fee, params.fee);
}