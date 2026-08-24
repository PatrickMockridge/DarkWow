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
//! This module provides the ability to build Revoke calls to destroy commitments.
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
        pedersen_commitment_u64, poseidon_hash, BaseBlind, Blind, FuncId, MerkleNode, ScalarBlind, SecretKey, AssetId,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;
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
    /// Value of the commitment being revokeed
    pub value: u64,
    /// Token ID
    pub asset_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Commitment blind
    pub commitment_blind: pallas::Base,
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

        // tx_binding/tx_nonce must match what the proof derived (create_revoke_proof uses
        // input.tx_commitment/tx_nonce); all inputs in one tx share the same binding.
        let tx_commitment = self.inputs[0].tx_commitment;
        let tx_nonce = self.inputs[0].tx_nonce;

        for input in self.inputs.into_iter() {
            // Generate revoke proof
            let (value_blind, asset_id_blind, user_data_blind) =
                if crate::deterministic_zk_enabled() {
                let mut rng = rand::rngs::StdRng::seed_from_u64(0);
                (ScalarBlind::random(&mut rng), BaseBlind::random(&mut rng),
                 BaseBlind::random(&mut rng))
            } else {
                (ScalarBlind::random(&mut OsRng), BaseBlind::random(&mut OsRng),
                 BaseBlind::random(&mut OsRng))
            };

            let (proof, revealed) = create_revoke_proof(
                &self.revoke_zkbin,
                &self.revoke_pk,
                &input,
                value_blind,
                asset_id_blind,
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
            params: RevokeParamsV1 { inputs, tx_binding: poseidon_hash([pallas::Base::from(3), tx_commitment, tx_nonce]), tx_nonce },
            proofs,
        })
    }
}

/// Create a ZK proof for revokeing (destroying) a commitment.
/// Value commitment: Pedersen (additively homomorphic).
pub fn create_revoke_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RevokeCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, RevokeRevealed)> {
    // Derive public key from secret using Poseidon (Schnorr-style).
    // V2 circuit domain separator: DOMAIN_SIGNATURE_SECRET = 7.
    let public_key = poseidon_hash([pallas::Base::from(7), input.secret]);

    let commitment = CapAttrs {
        public_key,
        value: input.value,
        asset_id: AssetId::from_base(input.asset_id),
        spend_hook: FuncId::from_base(input.spend_hook),
        user_data: input.user_data,
        blind: Blind(input.commitment_blind),
    }
    .to_commitment();

    // Calculate nullifier: poseidon_hash(secret, commitment)
    let nullifier = Nullifier::new(SecretKey::from_base(input.secret), commitment.inner());

    // Calculate merkle root from commitment and merkle path
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

    // Token ID commitment.
    // V2 circuit domain separator: DOMAIN_TOK_COMMIT = 2.
    let token_commit = poseidon_hash([pallas::Base::from(2), input.asset_id, asset_id_blind.inner()]);

    // User data encryption.
    // V2 circuit domain separator: DOMAIN_USER_DATA_ENC = 6.
    let user_data_enc = poseidon_hash([pallas::Base::from(6), input.user_data, user_data_blind.inner()]);

    // Derive per-revoke unique signature_secret from spend_secret + nullifier.
    // V2 circuit domain separator: DOMAIN_SIGNATURE_SECRET = 7.
    let signature_secret = poseidon_hash([pallas::Base::from(7), input.secret, nullifier.inner()]);
    let signature_public = poseidon_hash([pallas::Base::from(7), signature_secret]);

    let public_inputs = RevokeRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        user_data_enc,
        spend_hook: input.spend_hook,
        signature_public,
        tx_binding: poseidon_hash([pallas::Base::from(3), input.tx_commitment, input.tx_nonce]),
        tx_nonce: input.tx_nonce,
    };

    #[expect(clippy::unwrap_used, reason = "leaf position fits u32")]
    let leaf_position: u32 = u64::from(input.leaf_position).try_into().unwrap();
    #[expect(clippy::unwrap_used, reason = "merkle path length equals fixed tree depth")]
    let merkle_path = {
        let mut path = input.merkle_path.clone();
        if path.is_empty() {
            path.push(MerkleNode::from_bytes([0u8; 32])
                .unwrap_or_else(|| MerkleNode::new(pallas::Base::zero())));
        }
        path.try_into().unwrap()
    };
    let prover_witnesses = vec![
        Witness::Base(Value::known(input.secret)),
        Witness::Base(Value::known(pallas::Base::from(input.value))),
        Witness::Base(Value::known(input.asset_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.commitment_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(asset_id_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(leaf_position)),
        Witness::MerklePath(Value::known(merkle_path)),
        // Per-revoke signature_secret = poseidon_hash(spend_secret, nullifier).
        // Cryptographically bound to spend_secret (fixes H2) but unique per revoke
        // since each nullifier is unique — signature_public is unlinkable.
        Witness::Base(Value::known(signature_secret)),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3), input.tx_commitment, input.tx_nonce]))), // V2: tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce), domain = 3
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
