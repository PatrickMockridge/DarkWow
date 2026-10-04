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
        pedersen_commitment_u64, poseidon_hash, BaseBlind, MerkleNode, PublicKey, ScalarBlind, SecretKey, Blind, FuncId, AssetId,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;
use tracing::debug;

use super::PromissoryNote;
use crate::model::{AeadEncryptedNote, CapAttrs, CapCommitment, Input, Nullifier, Output, TransferParamsV1};

/// Public inputs revealed after burn proof (part of transfer)
/// Order must match Revoke_V1 circuit:
/// nullifier, value_commit_x, value_commit_y, token_commit, merkle_root,
/// user_data_enc, spend_hook, signature_public
pub struct TransferRevokeRevealed {
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

impl TransferRevokeRevealed {
    pub fn to_vec(&self) -> std::result::Result<Vec<pallas::Base>, crate::error::ContractError> {
        let (vc_x, vc_y) = point_to_coords(self.value_commit)?;
        Ok(vec![
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
        ])
    }
}

/// Public inputs revealed after blind output proof (part of transfer)
/// Order must match BlindOutput_V1 circuit:
/// commitment, value_commit_x, value_commit_y, token_commit, spend_hook
pub struct TransferBlindOutputRevealed {
    pub commitment: CapCommitment,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl TransferBlindOutputRevealed {
    pub fn to_vec(&self) -> std::result::Result<Vec<pallas::Base>, crate::error::ContractError> {
        let (vc_x, vc_y) = point_to_coords(self.value_commit)?;
        Ok(vec![
            self.commitment.inner(),
            vc_x,
            vc_y,
            self.token_commit,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ])
    }
}

/// Extract (x, y) base-field coordinates from a `pallas::Point`.
///
/// `Affine::coordinates()` returns a `subtle::CtOption` — invisible to `clippy::unwrap_used`,
/// which only sees `Option`/`Result` — and it is `None` exactly for the identity point, which has
/// no affine coordinates. A `pedersen_commitment_u64(0, Zero)` is the identity, so the former
/// `.unwrap()` was a reachable abort. Reject it as an error instead, stating the real condition.
fn point_to_coords(
    pt: pallas::Point,
) -> std::result::Result<(pallas::Base, pallas::Base), crate::error::ContractError> {
    let affine = pt.to_affine();
    let coords = affine.coordinates().into_option().ok_or_else(|| {
        crate::error::ContractError::IoError(
            "point_to_coords: value_commit is the identity point".to_string(),
        )
    })?;
    Ok((*coords.x(), *coords.y()))
}

/// Input commitment for transfer
#[derive(Clone)]
pub struct TransferCallInput {
    /// Value of the commitment being transferred
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
    /// Caller's secret key
    pub secret: pallas::Base,
    /// Ephemeral signature secret (Schnorr) — MUST be fresh per transaction.
    /// Never reuse the wallet secret here; doing so links all
    /// transfers to the same on-chain signature_public.
    pub ephemeral_signature_secret: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Output commitment for transfer
#[derive(Clone)]
pub struct TransferCallOutput {
    /// Recipient address (poseidon_hash of public key X coord)
    pub recipient: pallas::Base,
    /// Recipient's public key for AEAD note encryption (EC point for Diffie-Hellman)
    pub recipient_pub: PublicKey,
    /// Value to transfer
    pub value: u64,
    /// Token ID
    pub asset_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Commitment blind
    pub commitment_blind: pallas::Base,
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
    /// `Revoke_V1` zkas circuit ZkBinary
    pub revoke_zkbin: ZkBinary,
    /// Proving key for the `Revoke_V1` zk circuit
    pub revoke_pk: ProvingKey,
    /// `BlindOutput_V1` zkas circuit ZkBinary
    pub transfer_zkbin: ZkBinary,
    /// Proving key for the `BlindOutput_V1` zk circuit
    pub transfer_pk: ProvingKey,
    /// Optional caller-supplied value blinds (one per input/output pair). When
    /// set, overrides the internally generated pair blinds so a child transfer's
    /// output value_commit can match a parent's `validate_child_value_commit`.
    pub value_blinds: Option<Vec<ScalarBlind>>,
}

impl TransferCallBuilder {
    /// Build the Transfer call debris
    /// Build the call and prove it in one step, with the commitment the inputs carry.
    ///
    /// Kept for callers that already know theirs. A caller that does not — because the
    /// commitment is a derivation over the finished call set — uses [`Self::prepare`] and then
    /// [`TransferCallPlan::prove`] once the whole transaction is assembled (`OBL-C198`).
    pub fn build(self) -> Result<TransferCallDebris> {
        let commitment = self.inputs[0].tx_commitment;
        let nonce = self.inputs[0].tx_nonce;
        self.prepare()?.prove(commitment, nonce)
    }

    /// Assemble the call's data and stop — **before** the blind-output proofs exist.
    pub fn prepare(self) -> Result<TransferCallPlan> {
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

        let mut inputs = vec![];
        let mut outputs = vec![];
        let mut planned_inputs: Vec<PlannedTransferInput> = vec![];
        let mut planned_outputs: Vec<PlannedTransferOutput> = vec![];

        // Pre-generate value_blinds so burn and output proofs share the same
        // blind per input-output pair. Pedersen value conservation requires
        // equal value AND equal blind for matching input/output commitments.
        // Caller-supplied value_blinds (e.g. to satisfy a parent's
        // validate_child_value_commit) take precedence over generated blinds.
        let pair_blinds: Vec<ScalarBlind> = if let Some(blinds) = self.value_blinds {
            blinds
        } else if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            (0..self.inputs.len().max(self.outputs.len()))
                .map(|_| ScalarBlind::random(&mut rng))
                .collect()
        } else {
            (0..self.inputs.len().max(self.outputs.len()))
                .map(|_| ScalarBlind::random(&mut OsRng))
                .collect()
        };

        // Build burn proofs for inputs
        for (i, input) in self.inputs.clone().iter().enumerate() {
            let value_blind = pair_blinds[i].clone();
            // Deterministic asset_id_blind: same blind for all proofs of this asset_id,
            // so token_commit matches between burn and output for value conservation.
            let asset_id_blind = Blind(poseidon_hash([input.asset_id]));
            let user_data_blind = if crate::deterministic_zk_enabled() {
                BaseBlind::random(&mut rand::rngs::StdRng::seed_from_u64(0))
            } else {
                BaseBlind::random(&mut OsRng)
            };

            // Derive, do not prove. `revoke.zk` instances the tx pair — its last two instance
            // targets — so this proof is *bound* by the commitment and cannot be made until the
            // caller has derived it (`OBL-C198`). Making it here with the builder's own (zero)
            // commitment while the arm published the host's was the defect this fixes.
            let derived = derive_transfer_burn(
                input,
                value_blind.clone(),
                asset_id_blind.clone(),
                user_data_blind.clone(),
            );

            inputs.push(Input {
                value_commit: derived.value_commit,
                token_commit: derived.token_commit,
                nullifier: derived.nullifier,
                merkle_root: derived.merkle_root,
                user_data_enc: derived.user_data_enc,
                spend_hook: FuncId::from_base(input.spend_hook),
                signature_public: derived.signature_public,
            });

            planned_inputs.push(PlannedTransferInput {
                input: input.clone(),
                value_blind,
                asset_id_blind,
                user_data_blind,
            });
        }

        // Build blind output proofs for outputs
        for (i, output) in self.outputs.clone().iter().enumerate() {
            // Match the corresponding input's value_blind for Pedersen conservation.
            let value_blind = pair_blinds[i].clone();
            // Deterministic asset_id_blind: matches burn proof for value conservation.
            let asset_id_blind = Blind(poseidon_hash([output.asset_id]));

            // Derive, do not prove — the proof binds to a commitment derived over the finished
            // call data (`OBL-C198`), so the call is built here and proved in
            // `TransferCallPlan::prove`.
            let derived = derive_transfer_blind_output(output, value_blind.clone(), asset_id_blind.clone());

            planned_outputs.push(PlannedTransferOutput {
                output: output.clone(),
                value_blind: value_blind.clone(),
                asset_id_blind: asset_id_blind.clone(),
            });

            // Build note with all attributes the recipient needs to verify the commitment.
            // token_blind in the note must match asset_id_blind used in the ZK proof
            // so the recipient can independently verify the token_commit.
            let note = PromissoryNote {
                value: output.value,
                asset_id: output.asset_id,
                spend_hook: output.spend_hook,
                user_data: output.user_data,
                commitment_blind: output.commitment_blind,
                value_blind: value_blind.inner(),
                token_blind: asset_id_blind.inner(),
                memo: vec![],
                commitment: derived.commitment.inner(),
            };

            // Encrypt note to recipient's public key using AEAD (Diffie-Hellman + ChaCha20Poly1305).
            // Only the recipient (who holds the corresponding SecretKey) can decrypt it.
            let encrypted_note = if crate::deterministic_zk_enabled() {
                let mut rng = rand::rngs::StdRng::seed_from_u64(1);
                AeadEncryptedNote::encrypt(&note, &output.recipient_pub, &mut rng)
            } else {
                AeadEncryptedNote::encrypt(&note, &output.recipient_pub, &mut OsRng)
            }
            .map_err(|e| crate::error::ContractError::Custom({
                // Map SDK ContractError to a u32 error code for the promissory_note error type
                match e {
                    crate::error::ContractError::Custom(n) => n,
                    _ => u32::MAX,
                }
            }))?;

            outputs.push(Output {
                value_commit: derived.value_commit,
                token_commit: derived.token_commit,
                commitment: derived.commitment,
                note: encrypted_note,
                spend_hook: FuncId::from_base(output.spend_hook),
            });
        }

        // The params no longer carry the binding: `get_metadata` derives it from the commitment
        // the host exposes (`OBL-C198`), because a binding inside the call data would be computed
        // from a value that covers it. Only the nonce is carried.
        Ok(TransferCallPlan {
            revoke_zkbin: self.revoke_zkbin,
            revoke_pk: self.revoke_pk,
            transfer_zkbin: self.transfer_zkbin,
            transfer_pk: self.transfer_pk,
            planned_inputs,
            planned_outputs,
            inputs,
            outputs,
            tx_nonce: self.inputs[0].tx_nonce,
        })
    }
}

/// A transfer call whose data is assembled and whose blind-output proofs are not yet made.
///
/// The second half of the split `OBL-C198` forced, and it is the split that unblocks every
/// composition where this call is a **child**: the transaction commitment is a derivation over
/// the call data, so the call must exist before its proof — and it covers the *whole*
/// transaction's call set, not this call alone. A caller therefore builds every call it will
/// submit (parent and children), derives the commitment once over that ordered set, and only
/// then calls [`TransferCallPlan::prove`] with it, so parent and child bind to the same value.
pub struct TransferCallPlan {
    revoke_zkbin: ZkBinary,
    revoke_pk: ProvingKey,
    transfer_zkbin: ZkBinary,
    transfer_pk: ProvingKey,
    planned_inputs: Vec<PlannedTransferInput>,
    planned_outputs: Vec<PlannedTransferOutput>,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    tx_nonce: pallas::Base,
}

/// What a burn proof needs that the derivation does not produce.
struct PlannedTransferInput {
    input: TransferCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
}

/// What a blind-output proof needs that the derivation does not produce.
struct PlannedTransferOutput {
    output: TransferCallOutput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
}

impl TransferCallPlan {
    /// The contract's call parameters — no `tx_binding`, which `get_metadata` derives.
    pub fn params(&self) -> TransferParamsV1 {
        TransferParamsV1 {
            inputs: self.inputs.clone(),
            outputs: self.outputs.clone(),
            tx_nonce: self.tx_nonce,
        }
    }

    /// Prove every call in this plan, binding to `tx_commitment`.
    ///
    /// **Burn proofs first**, then the blind outputs — the order the old `build` produced, and
    /// both kinds are bound by the commitment. The burn proof takes the commitment from the
    /// input's own fields rather than as an argument, so each planned input is cloned and given
    /// the caller's value before proving.
    pub fn prove(
        self,
        tx_commitment: pallas::Base,
        tx_nonce: pallas::Base,
    ) -> Result<TransferCallDebris> {
        let mut proofs: Vec<Proof> = vec![];

        for p in &self.planned_inputs {
            let mut input = p.input.clone();
            input.tx_commitment = tx_commitment;
            input.tx_nonce = tx_nonce;
            let (burn_proof, _revealed) = create_transfer_burn_proof(
                &self.revoke_zkbin,
                &self.revoke_pk,
                &input,
                p.value_blind.clone(),
                p.asset_id_blind.clone(),
                p.user_data_blind.clone(),
            )?;
            proofs.push(burn_proof);
        }

        for p in &self.planned_outputs {
            let (transfer_proof, _revealed) = create_transfer_transfer_proof(
                &self.transfer_zkbin,
                &self.transfer_pk,
                &p.output,
                p.value_blind.clone(),
                p.asset_id_blind.clone(),
                tx_commitment,
                tx_nonce,
            )?;
            proofs.push(transfer_proof);
        }

        Ok(TransferCallDebris { params: self.params(), proofs })
    }
}

/// Create a burn proof for transfer.
/// Value commitment: Pedersen (additively homomorphic).
/// What a burn proof reveals, derived from its witnesses alone.
///
/// `OBL-C198`: the transaction commitment is a derivation over the call data, so the call has to
/// be assembled before its proof — and the call carries these values. None depends on the
/// commitment; `tx_binding` does, and is deliberately not here, so a caller can build the call
/// first and prove second.
///
/// **`revoke.zk` instances the tx pair**, as its last two instance targets — so unlike a
/// proof that merely *carries* the pair, this one is bound by it and cannot be made before the
/// commitment exists. That was the error this extraction fixes: `prepare` made the burn proofs
/// with the builder's own (zero) commitment while the arm published the host's.
///
/// One derivation, one home: `create_transfer_burn_proof` calls this (`safety.md` RC5).
pub struct TransferRevokeDerived {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub spend_hook: pallas::Base,
    pub signature_public: pallas::Base,
    /// Not a revealed value — an intermediate the witness vector needs, carried here so the
    /// proof function does not recompute the derivation it just called.
    pub signature_secret: pallas::Base,
}

/// Derive the burn's revealed values. Pure: no commitment, no proof, no randomness.
pub fn derive_transfer_burn(
    input: &TransferCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> TransferRevokeDerived {
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

    // Calculate nullifier
    let nullifier = Nullifier::new(SecretKey::from_base(input.secret), commitment.inner());

    // Calculate merkle root
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

    // Token commitment
    // V2 circuit domain separator: DOMAIN_TOK_COMMIT = 2.
    let token_commit = poseidon_hash([pallas::Base::from(2), input.asset_id, asset_id_blind.inner()]);

    // User data encryption.
    // V2 circuit domain separator: DOMAIN_USER_DATA_ENC = 6.
    let user_data_enc = poseidon_hash([pallas::Base::from(6), input.user_data, user_data_blind.inner()]);

    // Signature secret + public key.
    // V2 circuit derives signature_secret = H(7, spend_secret, nullifier) and
    // signature_public = H(7, signature_secret) — matches revoke.rs / revoke.zk.
    let signature_secret = poseidon_hash([pallas::Base::from(7), input.secret, nullifier.inner()]);
    let signature_public = poseidon_hash([pallas::Base::from(7), signature_secret]);

    TransferRevokeDerived {
        nullifier, value_commit, token_commit, merkle_root, user_data_enc,
        spend_hook: input.spend_hook, signature_public, signature_secret,
    }
}

fn create_transfer_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransferCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, TransferRevokeRevealed)> {
    let TransferRevokeDerived {
        nullifier, value_commit, token_commit, merkle_root, user_data_enc,
        spend_hook, signature_public, signature_secret,
    } = derive_transfer_burn(input, value_blind.clone(), asset_id_blind.clone(), user_data_blind.clone());

    let tx_binding = poseidon_hash([pallas::Base::from(3u64), input.tx_commitment, input.tx_nonce]);

    let public_inputs = TransferRevokeRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        user_data_enc,
        spend_hook: input.spend_hook,
        signature_public,
        tx_binding,
        tx_nonce: input.tx_nonce,
    };

    #[expect(clippy::unwrap_used, reason = "leaf position fits u32")]
    let leaf_position: u32 = u64::from(input.leaf_position).try_into().unwrap();
    #[expect(clippy::unwrap_used, reason = "merkle path length equals fixed tree depth")]
    let merkle_path = input.merkle_path.clone().try_into().unwrap();
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
        Witness::Base(Value::known(signature_secret)),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(tx_binding)), // tx_binding (shadowed, recomputed in-circuit)
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut OsRng)?;

    Ok((proof, public_inputs))
}

/// Create a blind output proof for transfer.
/// Uses BlindOutput_V1 circuit — proves the output commitment is well-formed without
/// requiring mint authority. Authorization comes from the burn side (nullifier
/// proves commitment ownership).
///
/// Now constrains token_commit so the entrypoint can group inputs and outputs
/// per token type for value conservation.
/// The values a blind-output proof reveals, derived from its witnesses alone.
///
/// `OBL-C198`: the transaction commitment is a derivation over the call data, so the call has
/// to be assembled before its proof — and the call carries these three values. None of them
/// depends on the commitment (`tx_binding` does, and is deliberately not here), so a caller can
/// build the call first and prove second.
///
/// One derivation, one home: `create_transfer_transfer_proof` calls this rather than repeating
/// it (`safety.md` RC5).
pub struct TransferBlindOutputDerived {
    pub commitment: CapCommitment,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
}

/// Derive the blind output's revealed values. Pure: no commitment, no proof, no randomness.
pub fn derive_transfer_blind_output(
    output: &TransferCallOutput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
) -> TransferBlindOutputDerived {
    // Create commitment attributes
    let attrs = CapAttrs {
        public_key: output.recipient,
        value: output.value,
        asset_id: AssetId::from_base(output.asset_id),
        spend_hook: FuncId::from_base(output.spend_hook),
        user_data: output.user_data,
        blind: Blind(output.commitment_blind),
    };
    let commitment = attrs.to_commitment();

    // Value commitment - Pedersen (additively homomorphic)
    let value_commit = pedersen_commitment_u64(output.value, value_blind.clone());

    // Token commitment - now ZK-constrained in BlindOutputV1
    // V2 circuit domain separator: DOMAIN_TOK_COMMIT = 2.
    let token_commit = poseidon_hash([pallas::Base::from(2), output.asset_id, asset_id_blind.inner()]);

    TransferBlindOutputDerived { commitment, value_commit, token_commit }
}

fn create_transfer_transfer_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &TransferCallOutput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, TransferBlindOutputRevealed)> {
    // Cloned rather than moved: the witnesses below still need both blinds.
    let TransferBlindOutputDerived { commitment, value_commit, token_commit } =
        derive_transfer_blind_output(output, value_blind.clone(), asset_id_blind.clone());

    let tx_binding = poseidon_hash([pallas::Base::from(3u64), tx_commitment, tx_nonce]);

    let public_inputs =
        TransferBlindOutputRevealed { commitment, value_commit, token_commit, spend_hook: output.spend_hook, tx_binding, tx_nonce };

    // Witness order must match BlindOutput_V1 circuit:
    // commitment_public, value, commitment_asset_id, commitment_spend_hook, commitment_user_data,
    // commitment_blind, value_blind, asset_id_blind
    let prover_witnesses = vec![
        Witness::Base(Value::known(output.recipient)),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output.asset_id)),
        Witness::Base(Value::known(output.spend_hook)),
        Witness::Base(Value::known(output.user_data)),
        Witness::Base(Value::known(output.commitment_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(asset_id_blind.inner())),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_nonce)),
        Witness::Base(Value::known(tx_binding)), // tx_binding (shadowed, recomputed in-circuit)
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut OsRng)?;

    Ok((proof, public_inputs))
}

#[cfg(test)]
mod tests {
    use dwow_sdk::{pasta::group::Group, pasta::pallas};

    use super::point_to_coords;

    /// R8 control: `Affine::coordinates()` is `None` for the identity point, so `point_to_coords`
    /// must return `Err`, never abort. The former `.unwrap()` was a `CtOption` the lint cannot see;
    /// this is the input it aborted on.
    #[test]
    fn identity_point_is_rejected() {
        assert!(
            point_to_coords(pallas::Point::identity()).is_err(),
            "the identity point has no affine coordinates; point_to_coords must return Err, never abort"
        );
    }
}
