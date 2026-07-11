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

//! Promissory Note MintV1 Client API
//!
//! This module provides the ability to build Mint calls to create new coins.
//! Uses Pedersen commitments for value — consistent with Transfer/Burn.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        pasta_prelude::{Curve, CurveAffine},
        pedersen_commitment_u64, poseidon_hash, Blind, FuncId, MerkleNode, ScalarBlind, TokenId,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{Coin, CoinAttributes, MintParamsV1};

/// Public inputs revealed after mint proof creation
/// Order must match Mint_V1 circuit:
/// token_root, mint_public, coin, value_commit_x, value_commit_y, token_id, spend_hook
pub struct MintRevealed {
    /// Merkle root of token registry
    pub token_registry_root: pallas::Base,
    /// Backing capability public key (poseidon_hash of backing secret)
    pub mint_public: pallas::Base,
    /// The coin commitment
    pub coin: Coin,
    /// The value commitment (Pedersen)
    pub value_commit: pallas::Point,
    /// The token ID
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Input for building a mint call
pub struct MintCallInput {
    /// Backing capability secret (proves right to mint this token)
    pub mint_secret: pallas::Base,
    /// Token registry Merkle tree leaf position
    pub token_leaf_pos: u32,
    /// Token registry Merkle path
    pub token_path: Vec<dwow_sdk::crypto::MerkleNode>,
    /// Recipient public key (poseidon_hash of secret)
    pub recipient: pallas::Base,
    /// Value to mint
    pub value: u64,
    /// Token ID (hidden commitment)
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blind
    pub coin_blind: pallas::Base,
}

/// Debris produced by building a Mint call, containing the parameters
/// and ZK proofs needed to execute the transaction.
pub struct MintCallDebris {
    /// The contract call parameters
    pub params: MintParamsV1,
    /// The ZK proofs for the mint operation
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `PromissoryNote::MintV1` contract call.
pub struct MintCallBuilder {
    /// The input for the mint
    pub input: MintCallInput,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl MintCallBuilder {
    /// Build the Mint call debris
    pub fn build(self) -> Result<MintCallDebris> {
        debug!(target: "contract::promissory_note::client::mint", "Building PromissoryNote::MintV1 contract call");

        // Derive mint_public from backing secret
        let mint_public = poseidon_hash([self.input.mint_secret]);

        // Generate blinds
        let value_blind = ScalarBlind::random(&mut OsRng);

        // Create coin attributes
        let attrs = CoinAttributes {
            public_key: self.input.recipient,
            value: self.input.value,
            token_id: TokenId(self.input.token_id),
            spend_hook: FuncId::from(self.input.spend_hook),
            user_data: self.input.user_data,
            blind: Blind(self.input.coin_blind),
        };

        // Create coin
        let coin = attrs.to_coin();

        // Value commitment - Pedersen (additively homomorphic)
        let value_commit = pedersen_commitment_u64(self.input.value, value_blind);

        // Calculate token_registry_root from Merkle path
        let token_registry_root = {
            let position: u64 = self.input.token_leaf_pos.into();
            let mut current = MerkleNode::from(self.input.token_id);
            for (level, sibling) in self.input.token_path.iter().enumerate() {
                let level = level as u8;
                current = if position & (1 << level) == 0 {
                    MerkleNode::combine(level.into(), &current, sibling)
                } else {
                    MerkleNode::combine(level.into(), sibling, &current)
                };
            }
            current
        };

        // Create prover witnesses
        let prover_witnesses = vec![
            // Backing capability proof
            // Note: mint_public is derived in-circuit as poseidon_hash(backing_secret),
            // so we pass the preimage (mint_secret) as witness[0].
            // mint_public itself is still witness[1] for constrain_instance exposure.
            Witness::Base(Value::known(self.input.mint_secret)),
            Witness::Base(Value::known(mint_public)),
            Witness::Uint32(Value::known(self.input.token_leaf_pos)),
            Witness::MerklePath(Value::known(self.input.token_path.clone().try_into().unwrap())),
            // Coin attributes
            Witness::Base(Value::known(self.input.recipient)),
            Witness::Base(Value::known(pallas::Base::from(self.input.value))),
            Witness::Base(Value::known(self.input.token_id)),
            Witness::Base(Value::known(self.input.spend_hook)),
            Witness::Base(Value::known(self.input.user_data)),
            Witness::Base(Value::known(self.input.coin_blind)),
            // Value commitment blind
            Witness::Scalar(Value::known(value_blind.inner())),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding computed in-circuit
        ];

        let public_inputs = MintRevealed {
            token_registry_root: token_registry_root.inner(),
            mint_public,
            coin,
            value_commit,
            token_id: TokenId(self.input.token_id),
            spend_hook: FuncId::from(self.input.spend_hook),
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        };

        let circuit = ZkCircuit::new(prover_witnesses, &self.mint_zkbin);
        let proof = Proof::create(&self.mint_pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

        Ok(MintCallDebris {
            params: MintParamsV1 {
                coin,
                value_commit,
                token_id: TokenId(self.input.token_id),
                token_registry_root,
                mint_public,
                spend_hook: FuncId::from(self.input.spend_hook),
            },
            proofs: vec![proof],
        })
    }
}

impl MintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = {
            let affine = self.value_commit.to_affine();
            let coords = affine.coordinates().unwrap();
            (*coords.x(), *coords.y())
        };
        vec![
            self.token_registry_root,
            self.mint_public,
            self.coin.inner(),
            vc_x,
            vc_y,
            self.token_id,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}
