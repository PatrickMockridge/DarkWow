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

//! Atomic Swap contract integration tests

use darkfi_atomic_swap_contract::{
    model::{
        chains, ClaimParamsV1, ClaimUpdateV1, CreateSwapParamsV1, CreateSwapUpdateV1,
        RefundParamsV1, RefundUpdateV1, Swap, SwapState,
    },
    AtomicSwapFunction,
    // Constants
    ATOMIC_SWAP_CONTRACT_INFO_TREE, ATOMIC_SWAP_CONTRACT_NULLIFIERS_TREE,
    ATOMIC_SWAP_CONTRACT_SECRETS_TREE, ATOMIC_SWAP_CONTRACT_SWAPS_TREE,
};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::{
    crypto::{PublicKey, SecretKey},
    pasta::pallas,
};

/// Helper to create PublicKey from a numeric seed
fn make_pubkey(seed: u64) -> PublicKey {
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

#[test]
fn test_atomic_swap_function_enum_valid() {
    assert!(AtomicSwapFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(AtomicSwapFunction::try_from(0x01).is_ok()); // CreateSwapV1
    assert!(AtomicSwapFunction::try_from(0x02).is_ok()); // ClaimV1
    assert!(AtomicSwapFunction::try_from(0x03).is_ok()); // RefundV1
}

#[test]
fn test_atomic_swap_function_enum_invalid() {
    assert!(AtomicSwapFunction::try_from(0xFF).is_err());
    assert!(AtomicSwapFunction::try_from(0x04).is_err());
    assert!(AtomicSwapFunction::try_from(0x10).is_err());
}

#[test]
fn test_swap_state_from_u8() {
    assert!(SwapState::try_from(0).is_ok());
    assert!(SwapState::try_from(1).is_ok());
    assert!(SwapState::try_from(2).is_ok());
    assert!(SwapState::try_from(3).is_ok());
    assert!(SwapState::try_from(4).is_err());
    assert!(SwapState::try_from(255).is_err());
}

#[test]
fn test_chain_constants() {
    assert_eq!(chains::CHAIN_ETHEREUM, 0);
    assert_eq!(chains::CHAIN_BITCOIN, 1);
    assert_eq!(chains::CHAIN_SOLANA, 2);
}

#[test]
fn test_swap_derive_id() {
    let hash = pallas::Base::from(12345);
    let timelock: u64 = 100000;
    let darkfi_receiver = make_pubkey(1);
    let amount: u64 = 1000;
    let token_id = pallas::Base::one();
    let side: u8 = 0;
    let blind = pallas::Base::zero();

    let swap_id = Swap::derive_id(
        hash,
        timelock,
        &darkfi_receiver,
        amount,
        token_id,
        side,
        blind,
    );

    // Swap ID should be deterministic
    let swap_id2 = Swap::derive_id(
        hash,
        timelock,
        &darkfi_receiver,
        amount,
        token_id,
        side,
        blind,
    );
    assert_eq!(swap_id, swap_id2);

    // Different inputs should produce different IDs
    let swap_id_different = Swap::derive_id(
        hash + pallas::Base::one(),
        timelock,
        &darkfi_receiver,
        amount,
        token_id,
        side,
        blind,
    );
    assert_ne!(swap_id, swap_id_different);
}

#[test]
fn test_swap_encoding() {
    let swap = Swap {
        id: pallas::Base::from(1),
        hash: pallas::Base::from(2),
        timelock: 100000,
        state: SwapState::Created,
        side: 0,
        external_chain: 0,
        external_receiver: pallas::Base::from(3),
        darkfi_receiver: make_pubkey(1),
        amount: 5000,
        token_id: pallas::Base::one(),
        blind: pallas::Base::zero(),
        created_at: 50000,
    };

    let encoded = serialize(&swap);
    let decoded: Swap = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, swap.id);
    assert_eq!(decoded.hash, swap.hash);
    assert_eq!(decoded.timelock, swap.timelock);
    assert_eq!(decoded.state, swap.state);
    assert_eq!(decoded.side, swap.side);
    assert_eq!(decoded.amount, swap.amount);
}

#[test]
fn test_create_swap_params_encoding() {
    let params = CreateSwapParamsV1 {
        hash: pallas::Base::from(1),
        timelock: 100000,
        side: 0,
        external_chain: 0,
        external_receiver: pallas::Base::from(2),
        darkfi_receiver: make_pubkey(1),
        amount: 5000,
        token_id: pallas::Base::one(),
        blind: pallas::Base::zero(),
        commitment: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    let decoded: CreateSwapParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.hash, params.hash);
    assert_eq!(decoded.timelock, params.timelock);
    assert_eq!(decoded.side, params.side);
    assert_eq!(decoded.amount, params.amount);
}

#[test]
fn test_create_swap_update_encoding() {
    let update = CreateSwapUpdateV1 {
        swap_id: pallas::Base::from(1),
        hash: pallas::Base::from(2),
        timelock: 100000,
        side: 1,
        external_chain: 1,
        external_receiver: pallas::Base::from(3),
        darkfi_receiver: make_pubkey(1),
        amount: 3000,
        token_id: pallas::Base::one(),
        blind: pallas::Base::zero(),
        created_at: 50000,
    };

    let encoded = serialize(&update);
    let decoded: CreateSwapUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, update.swap_id);
    assert_eq!(decoded.side, update.side);
    assert_eq!(decoded.amount, update.amount);
}

#[test]
fn test_claim_params_encoding() {
    let params = ClaimParamsV1 {
        swap_id: pallas::Base::from(1),
        secret: pallas::Base::from(2),
        nullifier: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    let decoded: ClaimParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, params.swap_id);
    assert_eq!(decoded.secret, params.secret);
    assert_eq!(decoded.nullifier, params.nullifier);
}

#[test]
fn test_claim_update_encoding() {
    let update = ClaimUpdateV1 {
        swap_id: pallas::Base::from(1),
        nullifier: pallas::Base::from(2),
        secret: pallas::Base::from(3),
    };

    let encoded = serialize(&update);
    let decoded: ClaimUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, update.swap_id);
    assert_eq!(decoded.nullifier, update.nullifier);
    assert_eq!(decoded.secret, update.secret);
}

#[test]
fn test_refund_params_encoding() {
    let params = RefundParamsV1 {
        swap_id: pallas::Base::from(1),
        current_block: 150000,
        nullifier: pallas::Base::from(2),
        recipient: make_pubkey(1),
    };

    let encoded = serialize(&params);
    let decoded: RefundParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, params.swap_id);
    assert_eq!(decoded.current_block, params.current_block);
    assert_eq!(decoded.nullifier, params.nullifier);
}

#[test]
fn test_refund_update_encoding() {
    let update = RefundUpdateV1 {
        swap_id: pallas::Base::from(1),
        nullifier: pallas::Base::from(2),
    };

    let encoded = serialize(&update);
    let decoded: RefundUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.swap_id, update.swap_id);
    assert_eq!(decoded.nullifier, update.nullifier);
}

#[test]
fn test_constants() {
    assert_eq!(ATOMIC_SWAP_CONTRACT_INFO_TREE, "info");
    assert_eq!(ATOMIC_SWAP_CONTRACT_SWAPS_TREE, "swaps");
    assert_eq!(ATOMIC_SWAP_CONTRACT_SECRETS_TREE, "secrets");
    assert_eq!(ATOMIC_SWAP_CONTRACT_NULLIFIERS_TREE, "nullifiers");
}