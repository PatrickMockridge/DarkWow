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

//! NativeToken BurnV1 Client API
//!
//! This module provides the ability to build Burn calls to destroy coins.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, BaseBlind, Keypair,
        MerkleNode, PublicKey, ScalarBlind, SecretKey,
    },
    error::ContractError,
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{BurnParamsV1, CoinAttributes, Input, Nullifier};

/// Public inputs revealed after burn proof creation
pub struct BurnRevealed {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub spend_hook: pallas::Base,
    pub signature_public: PublicKey,
}

impl BurnRevealed {
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

/// Create a ZK proof for burning (destroying) a coin.
#[allow(clippy::too_many_arguments)]
pub fn create_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &BurnCallInput,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    user_data_blind: BaseBlind,
    secret: SecretKey,
) -> Result<(Proof, BurnRevealed)> {
    let public_key = PublicKey::from_secret(secret);
    let signature_public = public_key;

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
    let nullifier = Nullifier::new(secret, coin.inner());

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

    let user_data_enc = poseidon_hash([input.user_data, user_data_blind.inner()]);
    let value_commit = pedersen_commitment_u64(input.value, value_blind);
    let token_commit = poseidon_hash([input.token_id, token_blind.inner()]);

    let public_inputs = BurnRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        spend_hook: input.spend_hook,
        user_data_enc,
        signature_public,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(input.value))),
        Witness::Base(Value::known(input.token_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
        Witness::Base(Value::known(secret.inner())),
        Witness::Base(Value::known(signature_public.x())),
        Witness::Base(Value::known(signature_public.y())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}

/// Struct holding necessary information to build a `NativeToken::BurnV1`
/// contract call.
pub struct BurnCallBuilder {
    /// Anonymous inputs
    pub inputs: Vec<BurnCallInput>,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
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
    pub merkle_path: Vec<dwow_sdk::crypto::MerkleNode>,
    /// Caller's keypair for signing
    pub keypair: Keypair,
}

/// Debris produced by building a Burn call, containing the parameters
/// and ZK proofs needed to execute the transaction.
pub struct BurnCallDebris {
    /// The contract call parameters
    pub params: BurnParamsV1,
    /// The ZK proofs for the burn operation
    pub proofs: Vec<Proof>,
    /// The ephemeral secret keys created for signing
    pub signature_secrets: Vec<SecretKey>,
}

impl BurnCallBuilder {
    /// Build the Burn call debris
    pub fn build(self) -> Result<BurnCallDebris> {
        debug!(target: "contract::native_token::client::burn", "Building NativeToken::BurnV1 contract call");

        if self.inputs.is_empty() {
            return Err(ContractError::Custom(1).into());
        }

        let mut proofs = vec![];
        let mut signature_secrets = vec![];
        let mut inputs = vec![];

        for input in self.inputs.into_iter() {
            let secret = input.keypair.secret;
            let signature_secret = secret;

            // Generate burn proof
            let value_blind = ScalarBlind::random(&mut OsRng);
            let token_blind = BaseBlind::random(&mut OsRng);
            let user_data_blind = BaseBlind::random(&mut OsRng);

            let (proof, _revealed) = create_burn_proof(
                &self.burn_zkbin,
                &self.burn_pk,
                &input,
                value_blind,
                token_blind,
                user_data_blind,
                secret,
            )?;

            proofs.push(proof);
            signature_secrets.push(signature_secret);

            // Create the Input model for params
            let coin = CoinAttributes {
                public_key: PublicKey::from_secret(secret),
                value: input.value,
                token_id: input.token_id,
                spend_hook: input.spend_hook,
                user_data: input.user_data,
                blind: input.coin_blind,
            }
            .to_coin();

            let value_commit = pedersen_commitment_u64(input.value, value_blind);
            let token_commit = poseidon_hash([input.token_id, token_blind.inner()]);
            let nullifier = Nullifier::new(secret, coin.inner());

            // Calculate merkle root
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

            let user_data_enc = poseidon_hash([input.user_data, user_data_blind.inner()]);

            inputs.push(Input {
                value_commit,
                token_commit,
                nullifier,
                merkle_root,
                user_data_enc,
                signature_public: PublicKey::from_secret(signature_secret),
                value: input.value,
                token_id: input.token_id,
                spend_hook: input.spend_hook,
                user_data: input.user_data,
                coin_blind: input.coin_blind,
                leaf_position: input.leaf_position,
                merkle_path: input.merkle_path,
            });
        }

        Ok(BurnCallDebris {
            params: BurnParamsV1 { inputs },
            proofs,
            signature_secrets,
        })
    }
}
