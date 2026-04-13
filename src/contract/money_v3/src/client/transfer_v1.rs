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
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Money V3 TransferV1 Client API
//!
//! This module provides the ability to build Transfer calls for private token transfers.
//! Transfer is an atomic burn + mint operation that preserves privacy.

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

use crate::model::{Coin, CoinAttributes, Input, Nullifier, Output, TransferParamsV1};

/// Public inputs revealed after burn proof (part of transfer)
pub struct TransferBurnRevealed {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Base,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub signature_public: pallas::Base,
}

impl TransferBurnRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.nullifier.inner(),
            self.value_commit,
            self.token_commit,
            self.merkle_root.inner(),
            self.user_data_enc,
            self.signature_public,
        ]
    }
}

/// Public inputs revealed after mint proof (part of transfer)
pub struct TransferMintRevealed {
    pub coin: Coin,
    pub value_commit: pallas::Base,
}

impl TransferMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.coin.inner(),
            self.value_commit,
        ]
    }
}

/// Input coin for transfer
pub struct TransferCallInput {
    /// Value of the coin being transferred
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
    /// Caller's secret key
    pub secret: pallas::Base,
    /// Signature secret (Schnorr)
    pub signature_secret: pallas::Base,
}

/// Output coin for transfer
pub struct TransferCallOutput {
    /// Recipient public key
    pub recipient: pallas::Base,
    /// Value to transfer
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blind
    pub coin_blind: pallas::Base,
}

/// Debris produced by building a Transfer call
pub struct TransferCallDebris {
    /// The contract call parameters
    pub params: TransferParamsV1,
    /// The ZK proofs (burn proofs first, then mint proofs)
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `MoneyV3::TransferV1` contract call.
pub struct TransferCallBuilder {
    /// Anonymous inputs being spent
    pub inputs: Vec<TransferCallInput>,
    /// Anonymous outputs being created
    pub outputs: Vec<TransferCallOutput>,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl TransferCallBuilder {
    /// Build the Transfer call debris
    pub fn build(self) -> Result<TransferCallDebris> {
        debug!(target: "contract::money_v3::client::transfer", "Building MoneyV3::TransferV1 contract call");

        if self.inputs.is_empty() {
            return Err(crate::error::MoneyV3Error::TransferMissingInputs.into())
        }
        if self.outputs.is_empty() {
            return Err(crate::error::MoneyV3Error::TransferMissingOutputs.into())
        }

        let mut proofs = vec![];
        let mut inputs = vec![];
        let mut outputs = vec![];

        // Build burn proofs for inputs
        for input in self.inputs.clone() {
            let value_blind = BaseBlind::random(&mut OsRng);
            let token_id_blind = BaseBlind::random(&mut OsRng);
            let user_data_blind = BaseBlind::random(&mut OsRng);

            let (burn_proof, revealed) = create_transfer_burn_proof(
                &self.burn_zkbin,
                &self.burn_pk,
                &input,
                value_blind,
                token_id_blind,
                user_data_blind,
            )?;

            proofs.push(burn_proof);

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

        // Build mint proofs for outputs
        for output in self.outputs.clone() {
            let value_blind = BaseBlind::random(&mut OsRng);

            let (mint_proof, mint_coin) = create_transfer_mint_proof(
                &self.mint_zkbin,
                &self.mint_pk,
                &output,
                value_blind,
            )?;

            proofs.push(mint_proof);

            let token_commit = poseidon_hash([output.token_id, output.coin_blind]);
            let note = Default::default(); // Transfer doesn't need note encryption

            outputs.push(Output {
                value_commit: poseidon_hash([pallas::Base::from(output.value), value_blind.inner()]),
                token_commit,
                coin: mint_coin,
                note,
            });
        }

        Ok(TransferCallDebris {
            params: TransferParamsV1 { inputs, outputs },
            proofs,
        })
    }
}

/// Create a burn proof for transfer
fn create_transfer_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransferCallInput,
    value_blind: BaseBlind,
    token_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, TransferBurnRevealed)> {
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

    // Calculate nullifier
    let nullifier = Nullifier::new(input.secret, coin.inner());

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

    // Value commitment
    let value_commit = poseidon_hash([pallas::Base::from(input.value), value_blind.inner()]);

    // Token commitment
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);

    // User data encryption
    let user_data_enc = poseidon_hash([input.user_data, user_data_blind.inner()]);

    // Signature public key
    let signature_public = poseidon_hash([input.signature_secret]);

    let public_inputs = TransferBurnRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        user_data_enc,
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

/// Create a mint proof for transfer
fn create_transfer_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &TransferCallOutput,
    value_blind: BaseBlind,
) -> Result<(Proof, Coin)> {
    // Create coin attributes
    let attrs = CoinAttributes {
        public_key: output.recipient,
        value: output.value,
        token_id: output.token_id,
        spend_hook: output.spend_hook,
        user_data: output.user_data,
        blind: output.coin_blind,
    };
    let coin = attrs.to_coin();

    // Value commitment
    let value_commit = poseidon_hash([pallas::Base::from(output.value), value_blind.inner()]);

    let public_inputs = TransferMintRevealed {
        coin,
        value_commit,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(output.recipient)),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output.token_id)),
        Witness::Base(Value::known(output.spend_hook)),
        Witness::Base(Value::known(output.user_data)),
        Witness::Base(Value::known(output.coin_blind)),
        Witness::Base(Value::known(value_blind.inner())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, coin))
}