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

//! Money V3 RotateMintAuthorityV1 Client API
//!
//! This module provides the ability to rotate mint authority for a token type.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{poseidon_hash, MerkleNode},
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::RotateMintAuthorityParamsV1;

/// Public inputs revealed after rotate mint authority proof creation
pub struct RotateMintAuthorityRevealed {
    /// Old authority public key (proves current authority)
    pub old_mint_public: pallas::Base,
    /// New authority public key (new authority)
    pub new_mint_public: pallas::Base,
    /// Token registry Merkle root
    pub token_registry_root: MerkleNode,
    /// Token ID
    pub token_id: pallas::Base,
}

impl RotateMintAuthorityRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.old_mint_public,
            self.new_mint_public,
            self.token_registry_root.inner(),
            self.token_id,
        ]
    }
}

/// Input for building a rotate mint authority call
pub struct RotateMintAuthorityCallInput {
    /// Old mint secret (proves current authority)
    pub old_mint_secret: pallas::Base,
    /// New mint secret (establishes new authority)
    pub new_mint_secret: pallas::Base,
    /// Token ID whose authority is being rotated
    pub token_id: pallas::Base,
    /// Merkle tree leaf position
    pub leaf_pos: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<MerkleNode>,
}

/// Debris produced by building a RotateMintAuthority call
pub struct RotateMintAuthorityCallDebris {
    /// The contract call parameters
    pub params: RotateMintAuthorityParamsV1,
    /// The ZK proofs for the rotation
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `MoneyV3::RotateMintAuthorityV1` contract call.
pub struct RotateMintAuthorityCallBuilder {
    /// The input for the rotate operation
    pub input: RotateMintAuthorityCallInput,
    /// `RotateMintAuthority_V1` zkas circuit ZkBinary
    pub rotate_zkbin: ZkBinary,
    /// Proving key for the `RotateMintAuthority_V1` zk circuit
    pub rotate_pk: ProvingKey,
}

impl RotateMintAuthorityCallBuilder {
    /// Build the RotateMintAuthority call debris
    pub fn build(self) -> Result<RotateMintAuthorityCallDebris> {
        debug!(target: "contract::money_v3::client::rotate_mint_authority", "Building MoneyV3::RotateMintAuthorityV1 contract call");

        // Derive old public key from old secret
        let old_mint_public = poseidon_hash([self.input.old_mint_secret]);

        // Derive new public key from new secret
        let new_mint_public = poseidon_hash([self.input.new_mint_secret]);

        // Calculate Merkle root for token registry
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

        let public_inputs = RotateMintAuthorityRevealed {
            old_mint_public,
            new_mint_public,
            token_registry_root,
            token_id: self.input.token_id,
        };

        let prover_witnesses = vec![
            Witness::Base(Value::known(self.input.old_mint_secret)),
            Witness::Base(Value::known(self.input.new_mint_secret)),
            Witness::Base(Value::known(self.input.token_id)),
            Witness::Uint32(Value::known(u64::from(self.input.leaf_pos).try_into().unwrap())),
            Witness::MerklePath(Value::known(self.input.merkle_path.clone().try_into().unwrap())),
        ];

        let circuit = ZkCircuit::new(prover_witnesses, &self.rotate_zkbin);
        let proof = Proof::create(&self.rotate_pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

        Ok(RotateMintAuthorityCallDebris {
            params: RotateMintAuthorityParamsV1 {
                old_mint_public,
                new_mint_public,
                token_id: self.input.token_id,
                token_registry_root,
            },
            proofs: vec![proof],
        })
    }
}