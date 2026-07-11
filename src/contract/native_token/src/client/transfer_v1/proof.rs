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

//! Transfer proofs for NativeToken
//!
//! This module provides ZK proof creation for mint and burn operations.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, BaseBlind, Blind, FuncId,
        MerkleNode, PublicKey, ScalarBlind, SecretKey,
    },
    pasta::pallas,
};
use crate::circuit::CircuitPublicInputs;
use rand::rngs::OsRng;
#[cfg(not(target_arch = "wasm32"))]
use rand::SeedableRng;
use tracing::debug;

use super::{TransferCallInput, TransferCallOutput};
use crate::model::{Coin, CoinAttributes, InputWitness, Nullifier};

/// Public inputs revealed after mint proof creation
pub struct TransferMintRevealed {
    pub coin: Coin,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    /// Nullifier: nf = poseidon_hash(coin_secret, coin) — capability claim
    pub nullifier: pallas::Base,
    /// New cumulative value commitment (S_H = S_{H-1} + C_H, from circuit)
    pub new_cumulative_commit: pallas::Point,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl TransferMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        self.to_public_inputs()
    }
}

impl crate::circuit::CircuitPublicInputs for TransferMintRevealed {
    const COUNT: usize = 9;

    fn to_public_inputs(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates()
            .expect("Value commitment cannot be the identity element");
        let cumcom_coords = self.new_cumulative_commit.to_affine().coordinates()
            .expect("Cumulative commitment cannot be the identity element");
        vec![
            self.coin.inner(),                  // 1: C
            self.nullifier,                     // 2: nf  (FA-1 fix — was missing)
            *valcom_coords.x(),                 // 3: vc.x
            *valcom_coords.y(),                 // 4: vc.y
            self.token_commit,                  // 5: tc
            *cumcom_coords.x(),                 // 6: S_H.x
            *cumcom_coords.y(),                 // 7: S_H.y
            self.tx_binding,                    // 8: tx_binding
            self.tx_nonce,                      // 9: tx_nonce
        ]
    }
}

/// Public inputs revealed after burn proof creation
pub struct TransferBurnRevealed {
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub nullifier: Nullifier,
    pub merkle_root: MerkleNode,
    pub spend_hook: pallas::Base,
    pub user_data_enc: pallas::Base,
    pub signature_public: PublicKey,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl TransferBurnRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        self.to_public_inputs()
    }
}

impl crate::circuit::CircuitPublicInputs for TransferBurnRevealed {
    const COUNT: usize = 11;

    fn to_public_inputs(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates()
            .expect("Value commitment cannot be the identity element");
        vec![
            self.nullifier.inner(),             // 1
            *valcom_coords.x(),                 // 2
            *valcom_coords.y(),                 // 3
            self.token_commit,                  // 4
            self.merkle_root.inner(),           // 5
            self.user_data_enc,                 // 6
            self.spend_hook,                    // 7
            self.signature_public.x().expect("pk not identity"),          // 8
            self.signature_public.y().expect("pk not identity"),          // 9
            self.tx_binding,                    // 10
            self.tx_nonce,                      // 11
        ]
    }
}

/// Create a ZK proof for minting (creating) a new coin.
#[allow(clippy::too_many_arguments)]
pub fn create_transfer_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &TransferCallOutput,
    coin_secret: SecretKey,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    coin_blind: BaseBlind,
    old_cumulative_value: u64,
    old_cumulative_blind: pallas::Scalar,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, TransferMintRevealed)> {
    let value_commit = pedersen_commitment_u64(output.value, value_blind);
    let token_commit = poseidon_hash([output.token_id, token_blind.inner()]);
    let (pub_x, pub_y) = output.public_key.xy().expect("pk not identity");

    let coin_attrs = CoinAttributes {
            version: 0,
        public_key: output.public_key,
        value: output.value,
        token_id: output.token_id,
        spend_hook: FuncId::from(spend_hook),
        user_data,
        blind: coin_blind,
    };
    debug!(target: "contract::native_token::client::transfer::proof", "Created coin: {coin_attrs:?}");
    let coin = coin_attrs.to_coin();

    // Compute new cumulative commitment: S_H = S_{H-1} + C_H
    // old_cumulative = pedersen_commit(old_value, old_blind) — reconstructed in circuit
    // new_cumulative = old_cumulative + coin_value_commit — verified in circuit
    let new_cumulative_commit = {
        let old_cum = pedersen_commitment_u64(old_cumulative_value, Blind(old_cumulative_blind));
        old_cum + value_commit
    };
    let cumcom_coords = new_cumulative_commit.to_affine().coordinates()
        .expect("Cumulative commitment cannot be the identity element");

    // Compute nullifier: nf = poseidon_hash(coin_secret.inner(), coin)
    let nf = poseidon_hash([coin_secret.inner(), coin.inner()]);

    let tx_binding = poseidon_hash([tx_commitment, tx_nonce]);

    let public_inputs = TransferMintRevealed {
        coin, value_commit, token_commit, nullifier: nf,
        new_cumulative_commit, tx_binding, tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output.token_id)),
        Witness::Base(Value::known(spend_hook)),
        Witness::Base(Value::known(user_data)),
        Witness::Base(Value::known(coin_blind.inner())),
        // coin_secret — per-block derived key sk_H. Required for nullifier constraint.
        Witness::Base(Value::known(coin_secret.inner())),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_blind.inner())),
        // Cumulative supply chain witnesses
        Witness::Base(Value::known(pallas::Base::from(old_cumulative_value))),
        Witness::Scalar(Value::known(old_cumulative_blind)),
        Witness::Base(Value::known(*cumcom_coords.x())),
        Witness::Base(Value::known(*cumcom_coords.y())),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_nonce)),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding computed in-circuit
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}

/// Create a ZK proof for burning (destroying) a coin.
#[allow(clippy::too_many_arguments)]
pub fn create_transfer_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransferCallInput,
    witness: &InputWitness,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    user_data_blind: BaseBlind,
    secret: SecretKey,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, TransferBurnRevealed)> {
    let public_key = PublicKey::from_secret(secret);
    let signature_public = public_key;

    // Reconstruct coin from the witness data
    let coin = CoinAttributes {
            version: 0,
        public_key,
        value: witness.value,
        token_id: witness.token_id,
        spend_hook: FuncId::from(input.spend_hook),
        user_data: witness.user_data,
        blind: witness.coin_blind,
    }
    .to_coin();

    // Calculate nullifier: poseidon_hash(secret, coin)
    let nullifier = Nullifier::new(secret, coin.inner());

    // Calculate merkle root from coin and merkle path
    let merkle_root = {
        let position: u64 = witness.leaf_position;
        let mut current = MerkleNode::from(coin.inner());
        for (level, sibling) in witness.merkle_path.iter().enumerate() {
            let level = level as u8;
            current = if position & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, sibling)
            } else {
                MerkleNode::combine(level.into(), sibling, &current)
            };
        }
        current
    };

    let public_inputs = TransferBurnRevealed {
        value_commit: input.value_commit,
        token_commit: input.token_commit,
        nullifier,
        merkle_root,
        spend_hook: input.spend_hook.inner(),
        user_data_enc: input.user_data_enc,
        signature_public,
        tx_binding: poseidon_hash([tx_commitment, tx_nonce]),
        tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(witness.value))),
        Witness::Base(Value::known(witness.token_id)),
        Witness::Base(Value::known(input.spend_hook.inner())),
        Witness::Base(Value::known(witness.user_data)),
        Witness::Base(Value::known(witness.coin_blind.inner())),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(u64::from(witness.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(witness.merkle_path.clone().try_into().unwrap())),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_nonce)),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding computed in-circuit
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}