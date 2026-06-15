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

//! OTC Swap contract integration tests

use dwow_serial::{deserialize, serialize};
use dwow_sdk::{
    crypto::{MerkleNode, pasta_prelude::Group},
    pasta::pallas,
};
use dwow_otc_swap_contract::{
    model::{
        CancelSwapParamsV1, CancelSwapUpdateV1, CreateSwapParamsV1, CreateSwapUpdateV1,
        ExecuteSwapParamsV1, ExecuteSwapUpdateV1, FundSwapParamsV1, FundSwapUpdateV1,
        OtcSwap, SwapId, SwapState,
    },
    OtcSwapFunction,
    OTC_SWAP_CONTRACT_INFO_TREE, OTC_SWAP_CONTRACT_SWAPS_TREE,
    OTC_SWAP_CONTRACT_NULLIFIERS_TREE,
};

/// Helper to create a test PublicKey
fn make_pubkey(seed: u64) -> dwow_sdk::crypto::PublicKey {
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

#[test]
fn test_otc_swap_function_enum_valid() {
    assert!(OtcSwapFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(OtcSwapFunction::try_from(0x01).is_ok()); // CreateSwapV1
    assert!(OtcSwapFunction::try_from(0x02).is_ok()); // FundSwapV1
    assert!(OtcSwapFunction::try_from(0x03).is_ok()); // ExecuteSwapV1
    assert!(OtcSwapFunction::try_from(0x04).is_ok()); // CancelSwapV1
}

#[test]
fn test_otc_swap_function_enum_invalid() {
    assert!(OtcSwapFunction::try_from(0xFF).is_err());
    assert!(OtcSwapFunction::try_from(0x05).is_err());
    assert!(OtcSwapFunction::try_from(0x10).is_err());
}

#[test]
fn test_swap_state_values() {
    assert_eq!(SwapState::Created as u8, 0);
    assert_eq!(SwapState::Funded as u8, 1);
    assert_eq!(SwapState::Executed as u8, 2);
    assert_eq!(SwapState::Cancelled as u8, 3);
}

#[test]
fn test_swap_state_try_from() {
    assert_eq!(SwapState::try_from(0).ok(), Some(SwapState::Created));
    assert_eq!(SwapState::try_from(1).ok(), Some(SwapState::Funded));
    assert_eq!(SwapState::try_from(2).ok(), Some(SwapState::Executed));
    assert_eq!(SwapState::try_from(3).ok(), Some(SwapState::Cancelled));
    assert!(SwapState::try_from(4).is_err());
    assert!(SwapState::try_from(255).is_err());
}

#[test]
fn test_constants() {
    assert_eq!(OTC_SWAP_CONTRACT_INFO_TREE, "info");
    assert_eq!(OTC_SWAP_CONTRACT_SWAPS_TREE, "swaps");
    assert_eq!(OTC_SWAP_CONTRACT_NULLIFIERS_TREE, "nullifiers");
}

#[test]
fn test_create_swap_params_encoding() {
    let params = CreateSwapParamsV1 {
        alice_pubkey: make_pubkey(1),
        bob_pubkey: make_pubkey(2),
        send_value: 1000,
        send_token_id: pallas::Base::from(1),
        recv_value: 2000,
        recv_token_id: pallas::Base::from(2),
        timeout: 100,
        commitment: pallas::Base::from(42),
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&params);
    let decoded: CreateSwapParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.alice_pubkey, params.alice_pubkey);
    assert_eq!(decoded.bob_pubkey, params.bob_pubkey);
    assert_eq!(decoded.send_value, params.send_value);
    assert_eq!(decoded.send_token_id, params.send_token_id);
    assert_eq!(decoded.recv_value, params.recv_value);
    assert_eq!(decoded.recv_token_id, params.recv_token_id);
    assert_eq!(decoded.timeout, params.timeout);
}

#[test]
fn test_create_swap_update_encoding() {
    let update = CreateSwapUpdateV1 {
        swap_id: pallas::Base::from(1),
    };

    let encoded = serialize(&update);
    let decoded: CreateSwapUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, update.swap_id);
}

#[test]
fn test_fund_swap_params_encoding() {
    let params = FundSwapParamsV1 {
        swap_id: pallas::Base::from(1),
        value_commit: pallas::Point::identity(),
        merkle_proof: vec![pallas::Base::from(1), pallas::Base::from(2)],
        merkle_root: MerkleNode::from(pallas::Base::from(99)),
    };

    let encoded = serialize(&params);
    let decoded: FundSwapParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, params.swap_id);
    assert_eq!(decoded.merkle_proof.len(), 2);
}

#[test]
fn test_fund_swap_update_encoding() {
    let update = FundSwapUpdateV1 {
        swap_id: pallas::Base::from(1),
    };

    let encoded = serialize(&update);
    let decoded: FundSwapUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, update.swap_id);
}

#[test]
fn test_execute_swap_params_encoding() {
    let params = ExecuteSwapParamsV1 {
        swap_id: pallas::Base::from(1),
        bob_secret: pallas::Base::from(42),
        spent_nullifier: pallas::Base::from(50),
        alice_recipient: make_pubkey(3),
        bob_recipient: make_pubkey(4),
    };

    let encoded = serialize(&params);
    let decoded: ExecuteSwapParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, params.swap_id);
    assert_eq!(decoded.bob_secret, params.bob_secret);
    assert_eq!(decoded.spent_nullifier, params.spent_nullifier);
    assert_eq!(decoded.alice_recipient, params.alice_recipient);
    assert_eq!(decoded.bob_recipient, params.bob_recipient);
}

#[test]
fn test_execute_swap_update_encoding() {
    let update = ExecuteSwapUpdateV1 {
        swap_id: pallas::Base::from(1),
        spent_nullifier: pallas::Base::from(50),
    };

    let encoded = serialize(&update);
    let decoded: ExecuteSwapUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, update.swap_id);
    assert_eq!(decoded.spent_nullifier, update.spent_nullifier);
}

#[test]
fn test_cancel_swap_params_encoding() {
    let params = CancelSwapParamsV1 {
        swap_id: pallas::Base::from(1),
        alice_secret: pallas::Base::from(42),
        spent_nullifier: pallas::Base::from(50),
        current_block: 150,
        timeout: 100,
        recipient_pubkey: make_pubkey(3),
    };

    let encoded = serialize(&params);
    let decoded: CancelSwapParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, params.swap_id);
    assert_eq!(decoded.alice_secret, params.alice_secret);
    assert_eq!(decoded.current_block, params.current_block);
    assert_eq!(decoded.timeout, params.timeout);
    assert_eq!(decoded.recipient_pubkey, params.recipient_pubkey);
}

#[test]
fn test_cancel_swap_update_encoding() {
    let update = CancelSwapUpdateV1 {
        swap_id: pallas::Base::from(1),
        spent_nullifier: pallas::Base::from(50),
    };

    let encoded = serialize(&update);
    let decoded: CancelSwapUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, update.swap_id);
    assert_eq!(decoded.spent_nullifier, update.spent_nullifier);
}

#[test]
fn test_swap_encoding() {
    let swap = OtcSwap {

        version: 0,        id: pallas::Base::from(1),
        alice_pubkey: make_pubkey(2),
        bob_pubkey: make_pubkey(3),
        send_value: 1000,
        send_token_id: pallas::Base::from(1),
        recv_value: 2000,
        recv_token_id: pallas::Base::from(2),
        timeout: 100,
        state: SwapState::Funded,
        alice_value_commit: pallas::Point::identity(),
        alice_value_blind: pallas::Scalar::from(99),
        bob_value_commit: pallas::Point::identity(),
        spent_nullifier: pallas::Base::from(50),
        created_at: 50,
        funded_at: Some(55),
        instance_seed: [1u8; 32],
    };

    let encoded = serialize(&swap);
    let decoded: OtcSwap = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, swap.id);
    assert_eq!(decoded.alice_pubkey, swap.alice_pubkey);
    assert_eq!(decoded.bob_pubkey, swap.bob_pubkey);
    assert_eq!(decoded.send_value, swap.send_value);
    assert_eq!(decoded.send_token_id, swap.send_token_id);
    assert_eq!(decoded.recv_value, swap.recv_value);
    assert_eq!(decoded.recv_token_id, swap.recv_token_id);
    assert_eq!(decoded.timeout, swap.timeout);
    assert_eq!(decoded.state, swap.state);
    assert_eq!(decoded.created_at, swap.created_at);
    assert_eq!(decoded.funded_at, swap.funded_at);
}

#[test]
fn test_swap_derive_id() {
    let alice_pubkey = make_pubkey(1);
    let bob_pubkey = make_pubkey(2);
    let send_value = 1000u64;
    let send_token_id = pallas::Base::from(1);
    let recv_value = 2000u64;
    let recv_token_id = pallas::Base::from(2);
    let timeout = 100u64;
    let alice_secret = pallas::Base::from(42);

    let swap_id: SwapId =
        OtcSwap::derive_id(&alice_pubkey, &bob_pubkey, send_value, send_token_id, recv_value, recv_token_id, timeout, alice_secret);

    assert!(swap_id != pallas::Base::zero());
}

#[test]
fn test_swap_compute_nullifier() {
    let swap = OtcSwap {

        version: 0,        id: pallas::Base::from(1),
        alice_pubkey: make_pubkey(2),
        bob_pubkey: make_pubkey(3),
        send_value: 1000,
        send_token_id: pallas::Base::from(1),
        recv_value: 2000,
        recv_token_id: pallas::Base::from(2),
        timeout: 100,
        state: SwapState::Funded,
        alice_value_commit: pallas::Point::identity(),
        alice_value_blind: pallas::Scalar::from(99),
        bob_value_commit: pallas::Point::identity(),
        spent_nullifier: pallas::Base::from(50),
        created_at: 50,
        funded_at: Some(55),
        instance_seed: [0u8; 32],
    };

    let secret = pallas::Base::from(42);
    let nullifier = swap.compute_nullifier(secret);

    assert!(nullifier != pallas::Base::zero());
}
