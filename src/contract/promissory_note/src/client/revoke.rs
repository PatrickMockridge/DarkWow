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

//! Promissory Note RevokeV1 Client API
//!
//! This module provides the ability to build Revoke calls to destroy coins.
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
        pedersen_commitment_u64, poseidon_hash, BaseBlind, Blind, FuncId, MerkleNode, ScalarBlind, TokenId,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{RevokeParamsV1, CapAttrs, Input, Nullifier};

/// Public inputs revealed after revoke proof creation
/// Order must match Revoke_V1 circuit:
/// nullifier, value_commit_x, value_commit_y, token_commit, merkle_root,
/// user_data_enc, spend_hook, signature_public
pub struct RevokeRevealed {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub spend_hook: pallas::Base,
    pub signature_public: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevokeRevealed {
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
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input for building a revoke call
pub struct RevokeCallInput {
    /// Value of the coin being revokeed
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
    /// revokes to the same on-chain signature_public.
    pub ephemeral_signature_secret: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Debris produced by building a Revoke call, containing the parameters
/// and ZK proofs needed to execute the transaction.
pub struct RevokeCallDebris {
    /// The contract call parameters
    pub params: RevokeParamsV1,
    /// The ZK proofs for the revoke operation
    pub proofs: Vec<Proof>,
}

/// Struct holding necessary information to build a `PromissoryNote::RevokeV1` contract call.
pub struct RevokeCallBuilder {
    /// Anonymous inputs
    pub inputs: Vec<RevokeCallInput>,
    /// `Revoke_V1` zkas circuit ZkBinary
    pub revoke_zkbin: ZkBinary,
    /// Proving key for the `Revoke_V1` zk circuit
    pub revoke_pk: ProvingKey,
}

impl RevokeCallBuilder {
    /// Build the Revoke call debris
    pub fn build(self) -> Result<RevokeCallDebris> {
        debug!(target: "contract::promissory_note::client::revoke", "Building PromissoryNote::RevokeV1 contract call");

        if self.inputs.is_empty() {
            return Err(crate::error::ContractError::Custom(
                crate::error::PromissoryNoteError::RevokeMissingInputs as u32,
            )
            .into());
        }

        let mut proofs = vec![];
        let mut inputs = vec![];

        for input in self.inputs.into_iter() {
            // Generate revoke proof
            let value_blind = ScalarBlind::random(&mut OsRng);
            let token_id_blind = BaseBlind::random(&mut OsRng);
            let user_data_blind = BaseBlind::random(&mut OsRng);

            let (proof, revealed) = create_revoke_proof(
                &self.revoke_zkbin,
                &self.revoke_pk,
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
                spend_hook: FuncId::from_base(input.spend_hook),
                signature_public: revealed.signature_public,
            });
        }

        Ok(RevokeCallDebris {
            params: RevokeParamsV1 { inputs, tx_binding: pallas::Base::zero(), tx_nonce: pallas::Base::zero() },
            proofs,
        })
    }
}

/// Create a ZK proof for revokeing (destroying) a coin.
/// Value commitment: Pedersen (additively homomorphic).
pub fn create_revoke_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RevokeCallInput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, RevokeRevealed)> {
    // Derive public key from secret using Poseidon (Schnorr-style)
    let public_key = poseidon_hash([input.secret]);

    let commitment = CapAttrs {
        public_key,
        value: input.value,
        token_id: TokenId::from_base(input.token_id),
        spend_hook: FuncId::from_base(input.spend_hook),
        user_data: input.user_data,
        blind: Blind(input.coin_blind),
    }
    .to_commitment();

    // Calculate nullifier: poseidon_hash(secret, coin)
    let nullifier = Nullifier::new(input.secret, commitment.inner());

    // Calculate merkle root from coin and merkle path
    let merkle_root = {
        let position: u64 = input.leaf_position.into();
        let mut current = MerkleNode::from_base(commitment.inner());
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
    let value_commit = pedersen_commitment_u64(input.value, value_blind.clone());

    // Token ID commitment
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);

    // User data encryption
    let user_data_enc = poseidon_hash([input.user_data, user_data_blind.inner()]);

    // Derive per-revoke unique signature_secret from coin_secret + nullifier.
    // poseidon_hash(coin_secret, nullifier) binds the signer to the coin owner
    // (fixes H2) while keeping signature_public unlinkable across revokes
    // (different nullifier → different signature_secret → different signature_public).
    let signature_secret = poseidon_hash([input.secret, nullifier.inner()]);
    let signature_public = poseidon_hash([signature_secret]);

    let public_inputs = RevokeRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        user_data_enc,
        spend_hook: input.spend_hook,
        signature_public,
        tx_binding: poseidon_hash([input.tx_commitment, input.tx_nonce]),
        tx_nonce: input.tx_nonce,
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
        Witness::MerklePath(Value::known({
            let mut path = input.merkle_path.clone();
            if path.is_empty() {
                path.push(MerkleNode::from_bytes([0u8; 32])
                    .unwrap_or_else(|| MerkleNode::new(pallas::Base::zero())));
            }
            path.try_into().unwrap()
        })),
        // Per-revoke signature_secret = poseidon_hash(coin_secret, nullifier).
        // Cryptographically bound to coin_secret (fixes H2) but unique per revoke
        // since each nullifier is unique — signature_public is unlinkable.
        Witness::Base(Value::known(signature_secret)),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(poseidon_hash([input.tx_commitment, input.tx_nonce]))), // V2: tx_binding = poseidon_hash(tx_commitment, tx_nonce)
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
