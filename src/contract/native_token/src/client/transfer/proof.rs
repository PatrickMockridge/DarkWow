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

//! Transfer proofs for NativeToken
//!
//! This module provides ZK proof creation for mint and burn operations.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        constants::{DRK_POSEIDON_DOMAIN_SIGNATURE_SECRET, DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, DRK_POSEIDON_DOMAIN_TX_BINDING},
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, BaseBlind, Blind, FuncId,
        MerkleNode, PublicKey, ScalarBlind, SecretKey, AssetId,
    },
    pasta::pallas,
};
use crate::circuit::CircuitPublicInputs;
use rand::rngs::OsRng;
#[cfg(not(target_arch = "wasm32"))]
use rand::SeedableRng;
use tracing::debug;

use super::{TransferCallInput, TransferCallOutput};
use crate::model::{Commitment, CommitmentAttributes, InputWitness, Nullifier};

/// Public inputs revealed after mint proof creation
pub struct TransferMintRevealed {
    pub commitment: Commitment,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    /// Nullifier: nf = poseidon_hash(spend_secret, commitment) — capability claim
    pub nullifier: pallas::Base,
    /// New cumulative value commitment (S_H = S_{H-1} + C_H, from circuit)
    pub new_cumulative_commit: pallas::Point,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    /// Reduced spendable note value (public input). Spec: uncle_merkle.md
    /// §"Spendable-note mass balance" — binds the actually-spendable note to the
    /// consensus reward, so a miner cannot mint full base and emit uncle notes.
    pub effective_value: u64,
}

impl TransferMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        self.to_public_inputs()
    }
}

impl crate::circuit::CircuitPublicInputs for TransferMintRevealed {
    const COUNT: usize = 10;

    fn to_public_inputs(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates()
            .expect("Value commitment cannot be the identity element");
        let cumcom_coords = self.new_cumulative_commit.to_affine().coordinates()
            .expect("Cumulative commitment cannot be the identity element");
        vec![
            self.commitment.inner(),                  // 1: C
            self.nullifier,                     // 2: nf  (FA-1 fix — was missing)
            *valcom_coords.x(),                 // 3: vc.x
            *valcom_coords.y(),                 // 4: vc.y
            self.token_commit,                  // 5: tc
            *cumcom_coords.x(),                 // 6: S_H.x
            *cumcom_coords.y(),                 // 7: S_H.y
            self.tx_binding,                    // 8: tx_binding
            self.tx_nonce,                      // 9: tx_nonce
            pallas::Base::from(self.effective_value), // 10: effective_value
        ]
    }
}

/// Public inputs revealed after burn proof creation
pub struct TransferBurnRevealed {
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub nullifier: Nullifier,
    pub merkle_root: MerkleNode,
    pub spend_hook: pallas::Base,
    pub user_data_enc: pallas::Base,
    pub signature_public: PublicKey,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl TransferBurnRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        self.to_public_inputs()
    }
}

impl crate::circuit::CircuitPublicInputs for TransferBurnRevealed {
    const COUNT: usize = 11;

    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so x()/y() is always Some")]
    fn to_public_inputs(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates()
            .expect("Value commitment cannot be the identity element");
        vec![
            self.nullifier.inner(),             // 1
            *valcom_coords.x(),                 // 2
            *valcom_coords.y(),                 // 3
            self.token_commit,                  // 4
            self.merkle_root.inner(),           // 5
            self.user_data_enc,                 // 6
            self.spend_hook,                    // 7
            self.signature_public.x().expect("pk not identity"),          // 8
            self.signature_public.y().expect("pk not identity"),          // 9
            self.tx_binding,                    // 10
            self.tx_nonce,                      // 11
        ]
    }
}

/// Create a ZK proof for minting (creating) a new commitment.
#[allow(clippy::too_many_arguments)]
pub fn create_transfer_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &TransferCallOutput,
    effective_value: u64,
    spend_secret: SecretKey,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    commitment_blind: BaseBlind,
    old_cumulative_value: u64,
    old_cumulative_blind: pallas::Scalar,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, TransferMintRevealed)> {
    let value_commit = pedersen_commitment_u64(output.value, value_blind.clone());
    let token_commit = poseidon_hash([DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, output.asset_id.inner(), token_blind.clone().inner()]);
    // Mint_V2 C1/C2 (M8): the commitment's public key is derived from spend_secret,
    // NOT from `output.public_key` (which is the note-encryption recipient).
    // Deriving from spend_secret satisfies `commitment_public == from_secret(spend_secret)`
    // so the mint proof is satisfiable regardless of who the recipient is.
    let commitment_public = PublicKey::from_secret(spend_secret.clone());
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
    let (pub_x, pub_y) = commitment_public.xy().expect("pk not identity");

    // Spec: uncle_merkle.md §Uncle Minting & Maturity — "Canonical note reduction":
    // the spendable note commits to effective_value (reduced), while value_commit
    // above still commits to the FULL value for the cumulative supply chain.
    let commitment_attrs = CommitmentAttributes {
            version: 0,
        public_key: commitment_public,
        value: effective_value,
        asset_id: output.asset_id,
        spend_hook: FuncId::from_base(spend_hook),
        user_data,
        blind: commitment_blind.clone(),
    };
    debug!(target: "contract::native_token::client::transfer::proof", "Created commitment: {commitment_attrs:?}");
    let commitment = commitment_attrs.to_commitment();

    // Compute new cumulative commitment: S_H = S_{H-1} + C_H
    // old_cumulative = pedersen_commit(old_value, old_blind) — reconstructed in circuit
    // new_cumulative = old_cumulative + value_commit — verified in circuit
    let new_cumulative_commit = {
        let old_cum = pedersen_commitment_u64(old_cumulative_value, Blind(old_cumulative_blind));
        old_cum + value_commit
    };
    let cumcom_coords = new_cumulative_commit.to_affine().coordinates()
        .expect("Cumulative commitment cannot be the identity element");

    // Compute nullifier: nf = poseidon_hash(DOMAIN_NULLIFIER, spend_secret, commitment)
    let nf = Nullifier::new(spend_secret.clone(), commitment.inner()).inner();

    let tx_binding = poseidon_hash([DRK_POSEIDON_DOMAIN_TX_BINDING, tx_commitment, tx_nonce]);

    let public_inputs = TransferMintRevealed {
        commitment, value_commit, token_commit, nullifier: nf,
        new_cumulative_commit, tx_binding, tx_nonce, effective_value,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(pallas::Base::from(effective_value))),
        Witness::Base(Value::known(output.asset_id.inner())),
        Witness::Base(Value::known(spend_hook)),
        Witness::Base(Value::known(user_data)),
        Witness::Base(Value::known(commitment_blind.clone().inner())),
        // spend_secret — per-block derived key sk_H. Required for nullifier constraint.
        Witness::Base(Value::known(*spend_secret.inner())),
        Witness::Scalar(Value::known(value_blind.clone().inner())),
        Witness::Base(Value::known(token_blind.clone().inner())),
        // Cumulative supply chain witnesses
        Witness::Base(Value::known(pallas::Base::from(old_cumulative_value))),
        Witness::Scalar(Value::known(old_cumulative_blind)),
        Witness::Base(Value::known(*cumcom_coords.x())),
        Witness::Base(Value::known(*cumcom_coords.y())),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_nonce)),
        // Mint proof: tx_binding computed in-circuit (mint_v2.zk), not as witness.
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

/// Create a ZK proof for burning (destroying) a commitment.
/// Returns (proof, revealed_public_inputs, per_burn_signature_secret).
#[allow(clippy::too_many_arguments)]
pub fn create_transfer_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransferCallInput,
    witness: &InputWitness,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    user_data_blind: BaseBlind,
    secret: SecretKey,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, TransferBurnRevealed, SecretKey)> {
    let public_key = PublicKey::from_secret(secret.clone());

    // Reconstruct commitment from the witness data
    let commitment = CommitmentAttributes {
            version: 0,
        public_key,
        value: witness.value,
        asset_id: AssetId::from_base(witness.asset_id),
        spend_hook: input.spend_hook,
        user_data: witness.user_data,
        blind: witness.commitment_blind.clone(),
    }
    .to_commitment();

    // Calculate nullifier: poseidon_hash(secret, commitment)
    let nullifier = Nullifier::new(secret.clone(), commitment.inner());

    // Derive per-burn unique signature_secret from spend_secret + nullifier.
    // This binds the signer to the commitment owner (fixes H2) while keeping
    // signature_public unlinkable across burns (nullifier is unique per commitment).
    let signature_secret = SecretKey::from_base(poseidon_hash([DRK_POSEIDON_DOMAIN_SIGNATURE_SECRET, *secret.inner(), nullifier.inner()]));
    let signature_public = PublicKey::from_secret(signature_secret.clone());

    // Calculate merkle root from commitment and merkle path
    let merkle_root = {
        let position: u64 = witness.leaf_position;
        let mut current = MerkleNode::from_base(commitment.inner());
        for (level, sibling) in witness.merkle_path.iter().enumerate() {
            let level = level as u8;
            current = if position & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, sibling)
            } else {
                MerkleNode::combine(level.into(), sibling, &current)
            };
        }
        current
    };

    let public_inputs = TransferBurnRevealed {
        value_commit: input.value_commit,
        token_commit: input.token_commit,
        nullifier,
        merkle_root,
        spend_hook: input.spend_hook.inner(),
        user_data_enc: input.user_data_enc,
        signature_public,
        tx_binding: poseidon_hash([DRK_POSEIDON_DOMAIN_TX_BINDING, tx_commitment, tx_nonce]),
        tx_nonce,
    };

    #[expect(clippy::unwrap_used, reason = "leaf position fits u32")]
    let leaf_position: u32 = u64::from(witness.leaf_position).try_into().unwrap();
    #[expect(clippy::unwrap_used, reason = "merkle path length equals fixed tree depth")]
    let merkle_path = witness.merkle_path.clone().try_into().unwrap();
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so x()/y() is always Some")]
    let sig_pub_x = signature_public.x().expect("pk not identity");
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so x()/y() is always Some")]
    let sig_pub_y = signature_public.y().expect("pk not identity");
    let prover_witnesses = vec![
        Witness::Base(Value::known(*secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(witness.value))),
        Witness::Base(Value::known(witness.asset_id)),
        Witness::Base(Value::known(input.spend_hook.inner())),
        Witness::Base(Value::known(witness.user_data)),
        Witness::Base(Value::known(BaseBlind::clone(&witness.commitment_blind).inner())),
        Witness::Scalar(Value::known(value_blind.clone().inner())),
        Witness::Base(Value::known(token_blind.clone().inner())),
        Witness::Base(Value::known(user_data_blind.clone().inner())),
        Witness::Uint32(Value::known(leaf_position)),
        Witness::MerklePath(Value::known(merkle_path)),
        // Per-burn signature_secret = poseidon_hash(spend_secret, nullifier).
        // Cryptographically bound to spend_secret (fixes H2) but unique per burn
        // (different nullifier → different signature_public — unlinkable).
        Witness::Base(Value::known(*signature_secret.inner())),
        Witness::Base(Value::known(sig_pub_x)),
        Witness::Base(Value::known(sig_pub_y)),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_nonce)),
        // tx_binding = poseidon_hash(tx_commitment, tx_nonce).
        // V2 circuits compute this in-circuit AND require it as a witness
        // for constraint satisfaction (17 witnesses total for burn_v2.zk).
        Witness::Base(Value::known(public_inputs.tx_binding)),
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

    Ok((proof, public_inputs, signature_secret))
}