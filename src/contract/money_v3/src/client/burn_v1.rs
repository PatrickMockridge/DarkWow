/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Money V3 BurnV1 Client API
//!
//! This module provides the ability to build Burn calls to destroy coins.
//! Uses Poseidon hash only - no EC operations.
//! Signature uses Schnorr-style where public_key = poseidon_hash(secret).

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    bridgetree::Hashable,
    crypto::{pasta_prelude::*, poseidon_hash, BaseBlind, MerkleNode},
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{BurnParamsV1, Coin, CoinAttributes, Input, Nullifier};

/// Public inputs revealed after burn proof creation
pub struct BurnRevealed {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Base,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub spend_hook: pallas::Base,
    pub signature_public: pallas::Base,
}

impl BurnRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.nullifier.inner(),
            self.value_commit,
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
    /// Signature secret (Schnorr)
    pub signature_secret: pallas::Base,
}

/// Debris produced by building a Burn call, containing the parameters
/// and ZK proofs needed to execute the transaction.
pub struct BurnCallDebris {
    /// The contract call parameters
    pub params: BurnParamsV1,
    /// The ZK proofs for the burn operation
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `MoneyV3::BurnV1` contract call.
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
        debug!(target: "contract::money_v3::client::burn", "Building MoneyV3::BurnV1 contract call");

        if self.inputs.is_empty() {
            return Err(crate::error::ContractError::Custom(
                crate::error::MoneyV3Error::BurnMissingInputs as u32,
            )
            .into());
        }

        let mut proofs = vec![];
        let mut inputs = vec![];

        for input in self.inputs.into_iter() {
            // Generate burn proof
            let value_blind = BaseBlind::random(&mut OsRng);
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
                signature_public: revealed.signature_public,
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
        })
    }
}

/// Create a ZK proof for burning (destroying) a coin.
/// Uses Poseidon hash only - no EC operations.
pub fn create_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &BurnCallInput,
    value_blind: BaseBlind,
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

    // Value commitment (Poseidon hash, not Pedersen)
    let value_commit = poseidon_hash([pallas::Base::from(input.value), value_blind.inner()]);

    // Token ID commitment
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);

    // User data encryption
    let user_data_enc = poseidon_hash([input.user_data, user_data_blind.inner()]);

    // Signature public key (Schnorr-style: poseidon_hash of secret)
    let signature_public = poseidon_hash([input.signature_secret]);

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
        Witness::Base(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
        Witness::Base(Value::known(input.signature_secret)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}