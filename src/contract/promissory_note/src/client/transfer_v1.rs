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

//! Promissory Note TransferV1 Client API
//!
//! This module provides the ability to build Transfer calls for private token transfers.
//! Transfer is an atomic burn + mint operation that preserves privacy.
//!
//! Value commitments use Pedersen (additively homomorphic) enabling the entrypoint
//! to enforce per-token-commit value conservation: sum(inputs) == sum(outputs).

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        pasta_prelude::{Curve, CurveAffine},
        pedersen_commitment_u64, poseidon_hash, BaseBlind, MerkleNode, PublicKey, ScalarBlind, Blind, FuncId, TokenId,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use super::PromissoryNote;
use crate::model::{AeadEncryptedNote, Coin, CoinAttributes, Input, Nullifier, Output, TransferParamsV1};

/// Public inputs revealed after burn proof (part of transfer)
/// Order must match Burn_V1 circuit:
/// nullifier, value_commit_x, value_commit_y, token_commit, merkle_root,
/// user_data_enc, spend_hook, signature_public
pub struct TransferBurnRevealed {
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

impl TransferBurnRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_to_coords(self.value_commit);
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

/// Public inputs revealed after blind output proof (part of transfer)
/// Order must match BlindOutput_V1 circuit:
/// coin, value_commit_x, value_commit_y, token_commit, spend_hook
pub struct TransferBlindOutputRevealed {
    pub coin: Coin,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl TransferBlindOutputRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_to_coords(self.value_commit);
        vec![
            self.coin.inner(),
            vc_x,
            vc_y,
            self.token_commit,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Extract (x, y) base-field coordinates from a pallas::Point.
fn point_to_coords(pt: pallas::Point) -> (pallas::Base, pallas::Base) {
    let affine = pt.to_affine();
    let coords = affine.coordinates().unwrap();
    (*coords.x(), *coords.y())
}

/// Input coin for transfer
#[derive(Clone)]
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
    /// Ephemeral signature secret (Schnorr) — MUST be fresh per transaction.
    /// Never reuse the wallet secret here; doing so links all
    /// transfers to the same on-chain signature_public.
    pub ephemeral_signature_secret: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Output coin for transfer
#[derive(Clone)]
pub struct TransferCallOutput {
    /// Recipient address (poseidon_hash of public key X coord)
    pub recipient: pallas::Base,
    /// Recipient's public key for AEAD note encryption (EC point for Diffie-Hellman)
    pub recipient_pub: PublicKey,
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

/// Struct holding necessary information to build a `PromissoryNote::TransferV1` contract call.
pub struct TransferCallBuilder {
    /// Anonymous inputs being spent
    pub inputs: Vec<TransferCallInput>,
    /// Anonymous outputs being created
    pub outputs: Vec<TransferCallOutput>,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
    /// `BlindOutput_V1` zkas circuit ZkBinary
    pub blind_output_zkbin: ZkBinary,
    /// Proving key for the `BlindOutput_V1` zk circuit
    pub blind_output_pk: ProvingKey,
}

impl TransferCallBuilder {
    /// Build the Transfer call debris
    pub fn build(self) -> Result<TransferCallDebris> {
        debug!(target: "contract::promissory_note::client::transfer", "Building PromissoryNote::TransferV1 contract call");

        if self.inputs.is_empty() {
            return Err(crate::error::ContractError::Custom(
                crate::error::PromissoryNoteError::TransferMissingInputs as u32,
            )
            .into());
        }
        if self.outputs.is_empty() {
            return Err(crate::error::ContractError::Custom(
                crate::error::PromissoryNoteError::TransferMissingOutputs as u32,
            )
            .into());
        }

        let mut proofs = vec![];
        let mut inputs = vec![];
        let mut outputs = vec![];

        // Build burn proofs for inputs
        for input in self.inputs.clone() {
            let value_blind = ScalarBlind::random(&mut OsRng);
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
                spend_hook: FuncId::from_base(input.spend_hook),
                signature_public: revealed.signature_public,
            });
        }

        // Build blind output proofs for outputs
        for output in self.outputs.clone() {
            let value_blind = ScalarBlind::random(&mut OsRng);
            let token_id_blind = BaseBlind::random(&mut OsRng);

            let (blind_output_proof, revealed) = create_transfer_blind_output_proof(
                &self.blind_output_zkbin,
                &self.blind_output_pk,
                &output,
                value_blind,
                token_id_blind,
                pallas::Base::zero(),
                pallas::Base::zero(),
            )?;

            proofs.push(blind_output_proof);

            // Build note with all attributes the recipient needs to verify the coin.
            // token_blind in the note must match token_id_blind used in the ZK proof
            // so the recipient can independently verify the token_commit.
            let note = PromissoryNote {
                value: output.value,
                token_id: output.token_id,
                spend_hook: output.spend_hook,
                user_data: output.user_data,
                coin_blind: output.coin_blind,
                value_blind: value_blind.inner(),
                token_blind: token_id_blind.inner(),
                memo: vec![],
            };

            // Encrypt note to recipient's public key using AEAD (Diffie-Hellman + ChaCha20Poly1305).
            // Only the recipient (who holds the corresponding SecretKey) can decrypt it.
            let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.recipient_pub, &mut OsRng)
                .map_err(|e| crate::error::ContractError::Custom({
                    // Map SDK ContractError to a u32 error code for the promissory_note error type
                    match e {
                        crate::error::ContractError::Custom(n) => n,
                        _ => u32::MAX,
                    }
                }))?;

            outputs.push(Output {
                value_commit: revealed.value_commit,
                token_commit: revealed.token_commit,
                coin: revealed.coin,
                note: encrypted_note,
                spend_hook: FuncId::from_base(output.spend_hook),
            });
        }

        Ok(TransferCallDebris {
            params: TransferParamsV1 { inputs, outputs,
                tx_binding: pallas::Base::zero(), tx_nonce: pallas::Base::zero() },
            proofs,
        })
    }
}

/// Create a burn proof for transfer.
/// Value commitment: Pedersen (additively homomorphic).
fn create_transfer_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransferCallInput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, TransferBurnRevealed)> {
    // Derive public key from secret using Poseidon (Schnorr-style)
    let public_key = poseidon_hash([input.secret]);

    // Reconstruct coin from the input
    let coin = CoinAttributes {
        public_key,
        value: input.value,
        token_id: TokenId::from_base(input.token_id),
        spend_hook: FuncId::from_base(input.spend_hook),
        user_data: input.user_data,
        blind: Blind(input.coin_blind),
    }
    .to_coin();

    // Calculate nullifier
    let nullifier = Nullifier::new(input.secret, coin.inner());

    // Calculate merkle root
    let merkle_root = {
        let position: u64 = input.leaf_position.into();
        let mut current = MerkleNode::from_base(coin.inner());
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
    let value_commit = pedersen_commitment_u64(input.value, value_blind);

    // Token commitment
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);

    // User data encryption
    let user_data_enc = poseidon_hash([input.user_data, user_data_blind.inner()]);

    // Signature public key
    let signature_public = poseidon_hash([input.ephemeral_signature_secret]);

    let public_inputs = TransferBurnRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        user_data_enc,
        spend_hook: input.spend_hook,
        signature_public,
        tx_binding: pallas::Base::zero(),
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
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
        Witness::Base(Value::known(input.ephemeral_signature_secret)),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding computed in-circuit
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}

/// Create a blind output proof for transfer.
/// Uses BlindOutput_V1 circuit — proves the output coin is well-formed without
/// requiring mint authority. Authorization comes from the burn side (nullifier
/// proves coin ownership).
///
/// Now constrains token_commit so the entrypoint can group inputs and outputs
/// per token type for value conservation.
fn create_transfer_blind_output_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &TransferCallOutput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, TransferBlindOutputRevealed)> {
    // Create coin attributes
    let attrs = CoinAttributes {
        public_key: output.recipient,
        value: output.value,
        token_id: TokenId::from_base(output.token_id),
        spend_hook: FuncId::from_base(output.spend_hook),
        user_data: output.user_data,
        blind: Blind(output.coin_blind),
    };
    let coin = attrs.to_coin();

    // Value commitment - Pedersen (additively homomorphic)
    let value_commit = pedersen_commitment_u64(output.value, value_blind);

    // Token commitment - now ZK-constrained in BlindOutputV1
    let token_commit = poseidon_hash([output.token_id, token_id_blind.inner()]);

    let public_inputs =
        TransferBlindOutputRevealed { coin, value_commit, token_commit, spend_hook: output.spend_hook, tx_binding: pallas::Base::zero(), tx_nonce };

    // Witness order must match BlindOutput_V1 circuit:
    // coin_public, coin_value, coin_token_id, coin_spend_hook, coin_user_data,
    // coin_blind, value_blind, token_id_blind
    let prover_witnesses = vec![
        Witness::Base(Value::known(output.recipient)),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output.token_id)),
        Witness::Base(Value::known(output.spend_hook)),
        Witness::Base(Value::known(output.user_data)),
        Witness::Base(Value::known(output.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_nonce)),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding computed in-circuit
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
