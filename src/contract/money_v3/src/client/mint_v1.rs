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

//! Money V3 MintV1 Client API
//!
//! This module provides the ability to build Mint calls to create new coins.
//! Uses Poseidon hash only - no EC operations.

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    bridgetree::Hashable,
    crypto::{pasta_prelude::*, poseidon_hash, BaseBlind, Keypair, MerkleNode},
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{AuthProof, Coin, CoinAttributes, MintParamsV1, Nullifier};

/// Public inputs revealed after mint proof creation
/// Order must match what Mint_V1 circuit expects: token_root, auth_nullifier,
/// auth_mint_public, coin, value_commit, token_id
pub struct MintRevealed {
    /// Merkle root of token registry
    pub token_registry_root: pallas::Base,
    /// Authorization nullifier
    pub auth_nullifier: pallas::Base,
    /// Authorization mint public key
    pub auth_mint_public: pallas::Base,
    /// The coin commitment
    pub coin: Coin,
    /// The value commitment (Poseidon hash)
    pub value_commit: pallas::Base,
    /// The token ID
    pub token_id: pallas::Base,
}

/// Input for building a mint call
pub struct MintCallInput {
    /// Authorization nullifier from AuthTokenMintV1
    pub auth_nullifier: pallas::Base,
    /// Authorization mint public key
    pub auth_mint_public: pallas::Base,
    /// Token registry Merkle tree leaf position
    pub token_leaf_pos: u32,
    /// Token registry Merkle path
    pub token_path: Vec<darkfi_sdk::crypto::MerkleNode>,
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

/// Struct holding necessary information to build a `MoneyV3::MintV1` contract call.
pub struct MintCallBuilder {
    /// The input for the mint
    pub input: MintCallInput,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl MintCallBuilder {
    /// Build the Mint call debris
    pub fn build(self) -> Result<MintCallDebris> {
        debug!(target: "contract::money_v3::client::mint", "Building MoneyV3::MintV1 contract call");

        // Generate blinds
        let value_blind = BaseBlind::random(&mut OsRng);

        // Create coin attributes
        let attrs = CoinAttributes {
            public_key: self.input.recipient,
            value: self.input.value,
            token_id: self.input.token_id,
            spend_hook: self.input.spend_hook,
            user_data: self.input.user_data,
            blind: self.input.coin_blind,
        };

        // Create coin
        let coin = attrs.to_coin();

        // Create value commitment (Poseidon hash, not Pedersen)
        let value_commit = poseidon_hash([pallas::Base::from(self.input.value), value_blind.inner()]);

        // Create token commitment
        let token_commit = poseidon_hash([self.input.token_id, self.input.coin_blind]);

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
            // Authorization data from AuthTokenMintV1
            Witness::Base(Value::known(self.input.auth_nullifier)),
            Witness::Base(Value::known(self.input.auth_mint_public)),
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
            Witness::Base(Value::known(value_blind.inner())),
        ];

        let public_inputs = MintRevealed {
            token_registry_root: token_registry_root.inner(),
            auth_nullifier: self.input.auth_nullifier,
            auth_mint_public: self.input.auth_mint_public,
            coin,
            value_commit,
            token_id: self.input.token_id,
        };

        let circuit = ZkCircuit::new(prover_witnesses, &self.mint_zkbin);
        let proof = Proof::create(&self.mint_pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

        Ok(MintCallDebris {
            params: MintParamsV1 {
                auth_proof: AuthProof {
                    nullifier: Nullifier::from_base(self.input.auth_nullifier),
                    mint_public: self.input.auth_mint_public,
                    token_registry_root,
                },
                coin,
                value_commit,
                token_id: self.input.token_id,
            },
            proofs: vec![proof],
        })
    }
}

impl MintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.token_registry_root,
            self.auth_nullifier,
            self.auth_mint_public,
            self.coin.inner(),
            self.value_commit,
            self.token_id,
        ]
    }
}