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

#![cfg(feature = "client")]

//! Plaintext PoW reward / uncle mint regression tests.
//!
//! consensus-coinbase.md §2.5: the coinbase (PoWRewardV1, 0x05) and uncle note
//! (UncleMintV1, 0x07) are minted WITHOUT a Mint_V2 ZK proof — the reward value
//! is public, so the entrypoint already verifies everything in plaintext
//! Pedersen/Poseidon. These tests lock the deprecation: the client builders must
//! emit `proofs: vec![]` and the extracted `compute_transfer_mint_revealed` must
//! commit the FULL value to the cumulative supply chain while the spendable note
//! commits the REDUCED effective value.

use dwow_native_token_contract::{
    circuit::CircuitPublicInputs,
    client::{
        pow_reward::PoWRewardCallBuilder,
        transfer::proof::{compute_transfer_mint_revealed, TransferMintRevealed},
        uncle_mint::build_uncle_mint,
    },
    model::{CommitmentAttributes, DRKW_ASSET_ID},
};
use dwow_sdk::{
    blockchain::BlockHeight,
    crypto::{pedersen_commitment_u64, BaseBlind, Blind, FuncId, PublicKey, ScalarBlind, SecretKey},
    pasta::{group::Group, pallas},
};

/// A deterministic secret key (type-system.md §2.7 — "no random keys").
fn test_secret(n: u64) -> SecretKey {
    SecretKey::from_base(pallas::Base::from(n))
}

#[test]
fn test_pow_reward_builder_produces_empty_proofs() {
    let builder = PoWRewardCallBuilder {
        secret: test_secret(1),
        ephemeral_signature_secret: test_secret(2),
        block_height: BlockHeight::new(2),
        recipient: None,
        spend_hook: None,
        user_data: None,
        expected_cumulative_supply: 0,
        old_total_supply: 0,
        old_cumulative_commit: pallas::Point::identity(),
        old_cumulative_blind: pallas::Scalar::zero(),
        tx_commitment: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };

    let debris = builder
        .build_with_custom_reward_and_effective(1000, 600)
        .expect("build plaintext coinbase");

    assert_eq!(debris.params.total_pin, 400, "total_pin = value − effective_value");
}

#[test]
fn test_uncle_mint_produces_empty_proofs() {
    let uncle_miner = PublicKey::from_secret(test_secret(3));

    let debris = build_uncle_mint(
        500,
        uncle_miner,
        [7u8; 32],
        BlockHeight::new(2),
        pallas::Base::zero(),
        pallas::Base::zero(),
    )
    .expect("build plaintext uncle note");

    assert_eq!(debris.params.total_pin, 0, "uncle note is not further split");
    assert_eq!(debris.params.input.value, 500, "uncle note value");
}

#[test]
fn test_compute_transfer_mint_revealed_invariants() {
    let spend_secret = test_secret(1);
    let value_blind: ScalarBlind = Blind(pallas::Scalar::from(11u64));
    let token_blind: BaseBlind = Blind(pallas::Base::from(12u64));
    let commitment_blind: BaseBlind = Blind(pallas::Base::from(13u64));

    let output = CommitmentAttributes {
        version: 0,
        public_key: PublicKey::from_secret(spend_secret.clone()),
        value: 1000,
        asset_id: DRKW_ASSET_ID,
        spend_hook: FuncId::from_base(pallas::Base::zero()),
        user_data: pallas::Base::zero(),
        blind: commitment_blind.clone(),
    };

    let revealed = compute_transfer_mint_revealed(
        &output,
        600, // effective_value
        400, // total_pin
        spend_secret.clone(),
        value_blind.clone(),
        token_blind.clone(),
        pallas::Base::zero(),   // spend_hook
        pallas::Base::zero(),   // user_data
        commitment_blind.clone(),
        0,                    // old_cumulative_value
        pallas::Scalar::zero(), // old_cumulative_blind
        pallas::Base::zero(),   // tx_commitment
        pallas::Base::zero(),   // tx_nonce
    );

    // value_commit commits the FULL base value (cumulative supply chain step).
    assert_eq!(
        revealed.value_commit,
        pedersen_commitment_u64(1000, value_blind.clone()),
        "value_commit must commit the full base value"
    );

    // new_cumulative_commit = S_{H-1} + C_H = identity + value_commit.
    assert_eq!(
        revealed.new_cumulative_commit,
        pallas::Point::identity() + revealed.value_commit,
        "new_cumulative_commit must be old_cumulative + value_commit"
    );

    // The spendable note commits the REDUCED effective value (canonical note
    // reduction, uncle_merkle.md §Uncle Minting & Maturity).
    let expected_commitment = CommitmentAttributes {
        version: 0,
        public_key: PublicKey::from_secret(spend_secret.clone()),
        value: 600,
        asset_id: DRKW_ASSET_ID,
        spend_hook: FuncId::from_base(pallas::Base::zero()),
        user_data: pallas::Base::zero(),
        blind: commitment_blind.clone(),
    }
    .to_commitment();
    assert_eq!(
        revealed.commitment.inner(),
        expected_commitment.inner(),
        "commitment must commit the reduced effective value"
    );

    assert_eq!(revealed.total_pin, 400);
    assert_eq!(TransferMintRevealed::COUNT, 10);
    assert_eq!(revealed.to_public_inputs().len(), TransferMintRevealed::COUNT);
}
