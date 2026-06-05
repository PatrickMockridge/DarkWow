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

//! Promissory Note BurnV1 Client API
//!
//! This module provides the ability to build Burn calls to destroy coins.
//! Uses Pedersen commitments for value — consistent with Transfer/Mint.
//! Signature uses Schnorr-style where public_key = poseidon_hash(secret).

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        pasta_prelude::{Curve, CurveAffine},
        pedersen_commitment_u64, poseidon_hash, BaseBlind, MerkleNode, ScalarBlind,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{BurnParamsV1, CoinAttributes, Input, Nullifier};

/// Public inputs revealed after burn proof creation
/// Order must match Burn_V1 circuit:
/// nullifier, value_commit_x, value_commit_y, token_commit, merkle_root,
/// user_data_enc, spend_hook, signature_public
pub struct BurnRevealed {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub spend_hook: pallas::Base,
    pub signature_public: pallas::Base,
}

impl BurnRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = {
            let affine = self.value_commit.to_affine();
            let coords = affine.coordinates().unwrap();
            (*coords.x(), *coords.y())
        };
        vec![
            self.nullifier.inner(),
            vc_x,
            vc_y,
            self.token_commit,
            self.merkle_root.inner(),
            self.user_data_enc,
            self.spend_hook,
            self.signature_public,
        ]
    }
}

/// Input for building a burn call
pub struct BurnCallInput {
    /// Value of the coin being burned
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blind
    pub coin_blind: pallas::Base,
    /// Merkle tree leaf position
    pub leaf_position: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<MerkleNode>,
    /// Caller's secret key (for Schnorr: public = poseidon_hash(secret))
    pub secret: pallas::Base,
    /// Ephemeral signature secret (Schnorr) — MUST be fresh per transaction.
    /// Never reuse the wallet secret here; doing so links all
    /// burns to the same on-chain signature_public.
    pub ephemeral_signature_secret: pallas::Base,
}

/// Debris produced by building a Burn call, containing the parameters
/// and ZK proofs needed to execute the transaction.
pub struct BurnCallDebris {
    /// The contract call parameters
    pub params: BurnParamsV1,
    /// The ZK proofs for the burn operation
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `PromissoryNote::BurnV1` contract call.
pub struct BurnCallBuilder {
    /// Anonymous inputs
    pub inputs: Vec<BurnCallInput>,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
}

impl BurnCallBuilder {
    /// Build the Burn call debris
    pub fn build(self) -> Result<BurnCallDebris> {
        debug!(target: "contract::promissory_note::client::burn", "Building PromissoryNote::BurnV1 contract call");

        if self.inputs.is_empty() {
            return Err(crate::error::ContractError::Custom(
                crate::error::PromissoryNoteError::BurnMissingInputs as u32,
            )
            .into());
        }

        let mut proofs = vec![];
        let mut inputs = vec![];

        for input in self.inputs.into_iter() {
            // Generate burn proof
            let value_blind = ScalarBlind::random(&mut OsRng);
            let token_id_blind = BaseBlind::random(&mut OsRng);
            let user_data_blind = BaseBlind::random(&mut OsRng);

            let (proof, revealed) = create_burn_proof(
                &self.burn_zkbin,
                &self.burn_pk,
                &input,
                value_blind,
                token_id_blind,
                user_data_blind,
            )?;

            proofs.push(proof);

            // Create the Input model for params
            inputs.push(Input {
                value_commit: revealed.value_commit,
                token_commit: revealed.token_commit,
                nullifier: revealed.nullifier,
                merkle_root: revealed.merkle_root,
                user_data_enc: revealed.user_data_enc,
                spend_hook: input.spend_hook,
                signature_public: revealed.signature_public,
            });
        }

        Ok(BurnCallDebris {
            params: BurnParamsV1 { inputs },
            proofs,
        })
    }
}

/// Create a ZK proof for burning (destroying) a coin.
/// Value commitment: Pedersen (additively homomorphic).
pub fn create_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &BurnCallInput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, BurnRevealed)> {
    // Derive public key from secret using Poseidon (Schnorr-style)
    let public_key = poseidon_hash([input.secret]);

    // Reconstruct coin from the input
    let coin = CoinAttributes {
        public_key,
        value: input.value,
        token_id: input.token_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.coin_blind,
    }
    .to_coin();

    // Calculate nullifier: poseidon_hash(secret, coin)
    let nullifier = Nullifier::new(input.secret, coin.inner());

    // Calculate merkle root from coin and merkle path
    let merkle_root = {
        let position: u64 = input.leaf_position.into();
        let mut current = MerkleNode::from(coin.inner());
        for (level, sibling) in input.merkle_path.iter().enumerate() {
            let level = level as u8;
            current = if position & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, sibling)
            } else {
                MerkleNode::combine(level.into(), sibling, &current)
            };
        }
        current
    };

    // Value commitment - Pedersen (additively homomorphic)
    let value_commit = pedersen_commitment_u64(input.value, value_blind);

    // Token ID commitment
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);

    // User data encryption
    let user_data_enc = poseidon_hash([input.user_data, user_data_blind.inner()]);

    // Signature public key is now derived from coin_secret in-circuit:
    // pub = poseidon_hash(coin_secret) is exposed as constrain_instance(pub).
    // This binds the transaction signer to the coin owner.
    let signature_public = poseidon_hash([input.secret]);

    let public_inputs = BurnRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        user_data_enc,
        spend_hook: input.spend_hook,
        signature_public,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.secret)),
        Witness::Base(Value::known(pallas::Base::from(input.value))),
        Witness::Base(Value::known(input.token_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
        // Note: signature_secret witness removed. The circuit now reuses
        // coin_secret for signing — pub = poseidon_hash(coin_secret) is
        // exposed as constrain_instance(pub).
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
