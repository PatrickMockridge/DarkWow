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
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, BaseBlind, MerkleNode,
        PublicKey, ScalarBlind, SecretKey,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use super::{TransferCallInput, TransferCallOutput};
use crate::model::{Coin, CoinAttributes, InputWitness, Nullifier};

/// Public inputs revealed after mint proof creation
pub struct TransferMintRevealed {
    pub coin: Coin,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
}

impl TransferMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().expect("Value commitment cannot be the identity element");
        vec![self.coin.inner(), *valcom_coords.x(), *valcom_coords.y(), self.token_commit]
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
}

impl TransferBurnRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().expect("Value commitment cannot be the identity element");
        vec![
            self.nullifier.inner(),
            *valcom_coords.x(),
            *valcom_coords.y(),
            self.token_commit,
            self.merkle_root.inner(),
            self.user_data_enc,
            self.spend_hook,
            self.signature_public.x(),
            self.signature_public.y(),
        ]
    }
}

/// Create a ZK proof for minting (creating) a new coin.
#[allow(clippy::too_many_arguments)]
pub fn create_transfer_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &TransferCallOutput,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    coin_blind: BaseBlind,
) -> Result<(Proof, TransferMintRevealed)> {
    let value_commit = pedersen_commitment_u64(output.value, value_blind);
    let token_commit = poseidon_hash([output.token_id, token_blind.inner()]);
    let (pub_x, pub_y) = output.public_key.xy();

    let coin_attrs = CoinAttributes {
        public_key: output.public_key,
        value: output.value,
        token_id: output.token_id,
        spend_hook,
        user_data,
        blind: coin_blind.inner(),
    };
    debug!(target: "contract::native_token::client::transfer::proof", "Created coin: {coin_attrs:?}");
    let coin = coin_attrs.to_coin();

    let public_inputs = TransferMintRevealed { coin, value_commit, token_commit };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output.token_id)),
        Witness::Base(Value::known(spend_hook)),
        Witness::Base(Value::known(user_data)),
        Witness::Base(Value::known(coin_blind.inner())),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_blind.inner())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
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
) -> Result<(Proof, TransferBurnRevealed)> {
    let public_key = PublicKey::from_secret(secret);
    let signature_public = public_key;

    // Reconstruct coin from the witness data
    let coin = CoinAttributes {
        public_key,
        value: witness.value,
        token_id: witness.token_id,
        spend_hook: input.spend_hook,
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
        spend_hook: input.spend_hook,
        user_data_enc: input.user_data_enc,
        signature_public,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(witness.value))),
        Witness::Base(Value::known(witness.token_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(witness.user_data)),
        Witness::Base(Value::known(witness.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(u64::from(witness.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(witness.merkle_path.clone().try_into().unwrap())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}