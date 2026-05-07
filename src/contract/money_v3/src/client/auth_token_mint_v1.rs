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

//! Money V3 AuthTokenMintV1 Client API
//!
//! This module provides the ability to authorize minting for an existing token type.

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    bridgetree::Hashable,
    crypto::{pasta_prelude::*, poseidon_hash, MerkleNode},
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{AuthTokenMintParamsV1, Nullifier};

/// Public inputs revealed after auth token mint proof creation
pub struct AuthTokenMintRevealed {
    /// Nullifier to prevent replay
    pub nullifier: Nullifier,
    /// Merkle root proving token_id exists
    pub token_registry_root: MerkleNode,
    /// Public key of the authority
    pub mint_public: pallas::Base,
}

impl AuthTokenMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.nullifier.inner(),
            self.token_registry_root.inner(),
            self.mint_public,
        ]
    }
}

/// Input for building an auth token mint call
pub struct AuthTokenMintCallInput {
    /// Secret key for signing
    pub mint_secret: pallas::Base,
    /// Token ID we are authorizing to mint
    pub token_id: pallas::Base,
    /// Merkle tree leaf position
    pub leaf_pos: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<MerkleNode>,
}

/// Debris produced by building an AuthTokenMint call
pub struct AuthTokenMintCallDebris {
    /// The contract call parameters
    pub params: AuthTokenMintParamsV1,
    /// The ZK proofs for the auth token mint operation
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `MoneyV3::AuthTokenMintV1` contract call.
pub struct AuthTokenMintCallBuilder {
    /// The input for the auth token mint
    pub input: AuthTokenMintCallInput,
    /// `AuthTokenMint_V1` zkas circuit ZkBinary
    pub auth_zkbin: ZkBinary,
    /// Proving key for the `AuthTokenMint_V1` zk circuit
    pub auth_pk: ProvingKey,
}

impl AuthTokenMintCallBuilder {
    /// Build the AuthTokenMint call debris
    pub fn build(self) -> Result<AuthTokenMintCallDebris> {
        debug!(target: "contract::money_v3::client::auth_token_mint", "Building MoneyV3::AuthTokenMintV1 contract call");

        // Derive public key from secret using Poseidon (Schnorr-style)
        let mint_public = poseidon_hash([self.input.mint_secret]);

        // Calculate nullifier: poseidon_hash(mint_secret, token_id)
        let nullifier = Nullifier::new_for_auth(self.input.mint_secret, self.input.token_id);

        // Calculate merkle root for token registry
        let token_registry_root = {
            let position: u64 = self.input.leaf_pos.into();
            let mut current = MerkleNode::from(self.input.token_id);
            for (level, sibling) in self.input.merkle_path.iter().enumerate() {
                let level = level as u8;
                current = if position & (1 << level) == 0 {
                    MerkleNode::combine(level.into(), &current, sibling)
                } else {
                    MerkleNode::combine(level.into(), sibling, &current)
                };
            }
            current
        };

        let public_inputs = AuthTokenMintRevealed {
            nullifier,
            token_registry_root,
            mint_public,
        };

        let prover_witnesses = vec![
            Witness::Base(Value::known(self.input.mint_secret)),
            Witness::Base(Value::known(self.input.token_id)),
            Witness::Uint32(Value::known(u64::from(self.input.leaf_pos).try_into().unwrap())),
            Witness::MerklePath(Value::known(self.input.merkle_path.clone().try_into().unwrap())),
        ];

        let circuit = ZkCircuit::new(prover_witnesses, &self.auth_zkbin);
        let proof = Proof::create(&self.auth_pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

        Ok(AuthTokenMintCallDebris {
            params: AuthTokenMintParamsV1 {
                nullifier,
                mint_public,
                token_id: self.input.token_id,
                token_registry_root,
            },
            proofs: vec![proof],
        })
    }
}