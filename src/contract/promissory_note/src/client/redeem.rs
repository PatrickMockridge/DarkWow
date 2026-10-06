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

//! Promissory Note RedeemV1 Client API
//!
//! RedeemV1 is the lifecycle close for a token's circulation. It burns the input
//! commitment (destroying monetary value) and creates a zero-value receipt commitment —
//! cryptographic proof that redemption occurred with the issuer.
//!
//! The receipt commitment is non-transferable (spend_hook = issuer contract) and serves
//! as both the redeemer's proof and the issuer's on-chain book-keeping record.

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
use crate::model::{AeadEncryptedNote, CapAttrs, CapCommitment, Input, Nullifier, Output, RedeemParamsV1};

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

// ============================================================================
// REVEALED PUBLIC INPUTS
// ============================================================================

/// Public inputs revealed after burn proof (redeem input side).
/// Order must match Burn_V1 circuit:
/// nullifier, value_commit_x, value_commit_y, token_commit, merkle_root,
/// user_data_enc, spend_hook, signature_public
pub struct RedeemRevokeRevealed {
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

impl RedeemRevokeRevealed {
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

/// Public inputs revealed after Redeem_V1 receipt proof.
/// Order must match Redeem_V1 circuit:
/// commitment, value_commit_x, value_commit_y, token_commit, value, spend_hook
pub struct RedeemReceiptRevealed {
    pub commitment: CapCommitment,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub value: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RedeemReceiptRevealed {
    pub fn to_vec(&self) -> std::result::Result<Vec<pallas::Base>, crate::error::ContractError> {
        let (vc_x, vc_y) = point_to_coords(self.value_commit)?;
        Ok(vec![
            self.commitment.inner(),
            vc_x,
            vc_y,
            self.token_commit,
            self.value,
            // `OBL-C198`: the pair is the last two instances, matching the reordered circuit — the
            // spend hook now precedes them.
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ])
    }
}

// ============================================================================
// BUILDER INPUTS
// ============================================================================

/// Input for redeeming a commitment.
pub struct RedeemCallInput {
    /// Value of the commitment being redeemed
    pub value: u64,
    /// Token ID
    pub asset_id: pallas::Base,
    /// Spend hook (issuer contract ID)
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
    /// Ephemeral signature secret — MUST be fresh per redemption.
    pub ephemeral_signature_secret: pallas::Base,
}

/// Output for the receipt commitment.
pub struct RedeemCallOutput {
    /// Recipient address (poseidon_hash of public key X coord)
    pub recipient: pallas::Base,
    /// Recipient's public key for AEAD note encryption
    pub recipient_pub: PublicKey,
    /// Token ID (same as redeemed commitment)
    pub asset_id: pallas::Base,
    /// Spend hook (issuer contract — makes receipt non-transferable)
    pub spend_hook: pallas::Base,
    /// User data (redemption metadata)
    pub user_data: pallas::Base,
    /// Commitment blind (fresh random per redemption)
    pub commitment_blind: pallas::Base,
}

// ============================================================================
// DEBRIS
// ============================================================================

/// Debris produced by building a Redeem call.
pub struct RedeemCallDebris {
    pub params: RedeemParamsV1,
    pub proofs: Vec<Proof>,
}

// ============================================================================
// BUILDER
// ============================================================================

/// Struct holding necessary information to build a `PromissoryNote::RedeemV1` contract call.
pub struct RedeemCallBuilder {
    /// Commitment being redeemed
    pub input: RedeemCallInput,
    /// Receipt commitment output
    pub output: RedeemCallOutput,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
    /// `Redeem_V1` zkas circuit ZkBinary (dedicated receipt circuit with is_notequal gate)
    pub redeem_zkbin: ZkBinary,
    /// Proving key for the `Redeem_V1` zk circuit
    pub redeem_pk: ProvingKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RedeemCallBuilder {
    /// Build the Redeem call debris — a burn proof for the input and a
    /// Redeem_V1 proof for the zero-value receipt.
    /// Build the call and prove it in one step, with the pair the builder carries.
    ///
    /// A caller that does not know its commitment — because it is a derivation over the finished
    /// call set — uses [`Self::prepare`] and then [`RedeemCallPlan::prove`] (`OBL-C198`).
    pub fn build(self) -> Result<RedeemCallDebris> {
        let commitment = self.tx_commitment;
        let nonce = self.tx_nonce;
        self.prepare()?.prove(commitment, nonce)
    }

    /// Assemble the params — including the AEAD note — and stop, **before** either proof exists
    /// (`OBL-C198`). The params do not carry `tx_binding`, so nothing here depends on the
    /// commitment; the two proofs do, and [`RedeemCallPlan::prove`] takes it.
    pub fn prepare(self) -> Result<RedeemCallPlan> {
        debug!(target: "contract::promissory_note::client::redeem", "Building PromissoryNote::RedeemV1 contract call");

        // Two separate seeded draws, kept exactly as `build` made them: collapsing them would move
        // every deterministic-zk value this path produces.
        let (value_blind, asset_id_blind, user_data_blind) =
            if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            (ScalarBlind::random(&mut rng), BaseBlind::random(&mut rng),
             BaseBlind::random(&mut rng))
        } else {
            (ScalarBlind::random(&mut OsRng), BaseBlind::random(&mut OsRng),
             BaseBlind::random(&mut OsRng))
        };

        let burn_derived = derive_redeem_burn(&self.input, &value_blind, &asset_id_blind, &user_data_blind);

        let input = Input {
            value_commit: burn_derived.value_commit,
            token_commit: burn_derived.token_commit,
            nullifier: burn_derived.nullifier,
            merkle_root: burn_derived.merkle_root,
            user_data_enc: burn_derived.user_data_enc,
            spend_hook: FuncId::from_base(self.input.spend_hook),
            signature_public: burn_derived.signature_public,
        };

        let (receipt_value_blind, receipt_asset_id_blind) =
            if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            (ScalarBlind::random(&mut rng), BaseBlind::random(&mut rng))
        } else {
            (ScalarBlind::random(&mut OsRng), BaseBlind::random(&mut OsRng))
        };

        let receipt_derived = derive_redeem_receipt(&self.output, &receipt_value_blind, &receipt_asset_id_blind);

        // Build note for the receipt so the redeemer can discover it via trial-decryption
        let note = PromissoryNote {
            value: 0,
            asset_id: self.output.asset_id,
            spend_hook: self.output.spend_hook,
            user_data: self.output.user_data,
            commitment_blind: self.output.commitment_blind,
            value_blind: receipt_value_blind.inner(),
            token_blind: receipt_asset_id_blind.inner(),
            memo: vec![],
            commitment: receipt_derived.commitment.inner(),
        };

        let encrypted_note = if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(1);
            AeadEncryptedNote::encrypt(&note, &self.output.recipient_pub, &mut rng)
        } else {
            AeadEncryptedNote::encrypt(&note, &self.output.recipient_pub, &mut OsRng)
        }
        .map_err(|e| crate::error::ContractError::Custom(match e {
            crate::error::ContractError::Custom(n) => n,
            _ => u32::MAX,
        }))?;

        let output = Output {
            value_commit: receipt_derived.value_commit,
            token_commit: receipt_derived.token_commit,
            commitment: receipt_derived.commitment,
            note: encrypted_note,
            spend_hook: FuncId::from_base(self.output.spend_hook),
        };

        Ok(RedeemCallPlan {
            burn_zkbin: self.burn_zkbin,
            burn_pk: self.burn_pk,
            redeem_zkbin: self.redeem_zkbin,
            redeem_pk: self.redeem_pk,
            input: self.input,
            output: self.output,
            value_blind, asset_id_blind, user_data_blind, burn_derived,
            receipt_value_blind, receipt_asset_id_blind, receipt_derived,
            params: RedeemParamsV1 { input, output,
                // `OBL-C198`: `tx_binding` left the params — the arm derives it from the host.
                tx_nonce: self.tx_nonce },
        })
    }
}

/// A `RedeemV1` call assembled but not yet proven (`OBL-C198`) — see
/// [`RedeemCallBuilder::prepare`] for why the split exists.
pub struct RedeemCallPlan {
    burn_zkbin: ZkBinary,
    burn_pk: ProvingKey,
    redeem_zkbin: ZkBinary,
    redeem_pk: ProvingKey,
    input: RedeemCallInput,
    output: RedeemCallOutput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
    burn_derived: RedeemRevokeDerived,
    receipt_value_blind: ScalarBlind,
    receipt_asset_id_blind: BaseBlind,
    receipt_derived: RedeemReceiptDerived,
    params: RedeemParamsV1,
}

impl RedeemCallPlan {
    /// The params — what the caller encodes into the call data the commitment is taken over.
    pub fn params(&self) -> RedeemParamsV1 {
        self.params.clone()
    }

    /// Prove against `tx_commitment` — the commitment over the whole ordered call set the node will
    /// hash, not just this call. `tx_nonce` must be the nonce the params carry, because the arm
    /// publishes that one and the proof's instance must agree with it.
    pub fn prove(self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<RedeemCallDebris> {
        let mut proofs = vec![];

        let (burn_proof, _burn_revealed) = create_redeem_burn_proof(
            &self.burn_zkbin,
            &self.burn_pk,
            &self.input,
            &self.value_blind,
            &self.asset_id_blind,
            &self.user_data_blind,
            &self.burn_derived,
            tx_commitment,
            tx_nonce,
        )?;
        proofs.push(burn_proof);

        let (output_proof, _output_revealed) = create_redeem_receipt_proof(
            &self.redeem_zkbin,
            &self.redeem_pk,
            &self.output,
            &self.receipt_value_blind,
            &self.receipt_asset_id_blind,
            &self.receipt_derived,
            tx_commitment,
            tx_nonce,
        )?;
        proofs.push(output_proof);

        Ok(RedeemCallDebris { params: self.params, proofs })
    }
}

// ============================================================================
// PROOF CREATION
// ============================================================================

/// What a redeem burn derivation produces that does **not** depend on the transaction commitment
/// (`OBL-C198`). `tx_binding` is the only pair-dependent value and is injected at prove time.
pub struct RedeemRevokeDerived {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub signature_secret: pallas::Base,
    pub signature_public: pallas::Base,
    leaf_position: u32,
    merkle_path: [MerkleNode; 32],
}

/// The burn derivation, lifted out of the proof so a caller can learn the params before it knows
/// the commitment they will be hashed into (`OBL-C198`). `prove_redeem_burn` calls this too.
pub fn derive_redeem_burn(
    input: &RedeemCallInput,
    value_blind: &ScalarBlind,
    asset_id_blind: &BaseBlind,
    user_data_blind: &BaseBlind,
) -> RedeemRevokeDerived {
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

    let nullifier = Nullifier::new(SecretKey::from_base(input.secret), commitment.inner());

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

    let value_commit = pedersen_commitment_u64(input.value, value_blind.clone());
    // V2 circuit domain separator: DOMAIN_TOK_COMMIT = 2.
    let token_commit = poseidon_hash([pallas::Base::from(2), input.asset_id, asset_id_blind.inner()]);
    // V2 circuit domain separator: DOMAIN_USER_DATA_ENC = 6.
    let user_data_enc = poseidon_hash([pallas::Base::from(6), input.user_data, user_data_blind.inner()]);
    // V2 circuit derives signature_secret = H(7, spend_secret, nullifier) and
    // signature_public = H(7, signature_secret) — matches revoke.rs / revoke.zk.
    let signature_secret = poseidon_hash([pallas::Base::from(7), input.secret, nullifier.inner()]);
    let signature_public = poseidon_hash([pallas::Base::from(7), signature_secret]);

    #[expect(clippy::unwrap_used, reason = "leaf position fits u32")]
    let leaf_position: u32 = u64::from(input.leaf_position).try_into().unwrap();
    #[expect(clippy::unwrap_used, reason = "merkle path length equals fixed tree depth")]
    let merkle_path = input.merkle_path.clone().try_into().unwrap();

    RedeemRevokeDerived {
        nullifier, value_commit, token_commit, merkle_root, user_data_enc,
        signature_secret, signature_public, leaf_position, merkle_path,
    }
}

/// Create a burn proof for the input commitment being redeemed.
/// Reuses the existing Burn_V1 circuit.
fn create_redeem_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RedeemCallInput,
    value_blind: &ScalarBlind,
    asset_id_blind: &BaseBlind,
    user_data_blind: &BaseBlind,
    derived: &RedeemRevokeDerived,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, RedeemRevokeRevealed)> {
    let tx_binding = poseidon_hash([pallas::Base::from(3u64), tx_commitment, tx_nonce]);

    let public_inputs = RedeemRevokeRevealed {
        nullifier: derived.nullifier,
        value_commit: derived.value_commit,
        token_commit: derived.token_commit,
        merkle_root: derived.merkle_root,
        user_data_enc: derived.user_data_enc,
        spend_hook: input.spend_hook,
        signature_public: derived.signature_public,
        tx_binding,
        tx_nonce,
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
        Witness::Uint32(Value::known(derived.leaf_position)),
        Witness::MerklePath(Value::known(derived.merkle_path.clone())),
        Witness::Base(Value::known(derived.signature_secret)),
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

/// What a redeem receipt derivation produces that does **not** depend on the transaction commitment
/// (`OBL-C198`). `commitment` is here because the AEAD note is encrypted over it and the note is
/// built before the proof exists.
pub struct RedeemReceiptDerived {
    pub commitment: CapCommitment,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub value: pallas::Base,
}

/// The receipt derivation, lifted out of the proof so the note and the params can be built before
/// the commitment is known (`OBL-C198`). `create_redeem_receipt_proof` calls this too.
pub fn derive_redeem_receipt(
    output: &RedeemCallOutput,
    value_blind: &ScalarBlind,
    asset_id_blind: &BaseBlind,
) -> RedeemReceiptDerived {
    let value = pallas::Base::zero();
    let attrs = CapAttrs {
        public_key: output.recipient,
        value: 0,
        asset_id: AssetId::from_base(output.asset_id),
        spend_hook: FuncId::from_base(output.spend_hook),
        user_data: output.user_data,
        blind: Blind(output.commitment_blind),
    };
    let commitment = attrs.to_commitment();

    let value_commit = pedersen_commitment_u64(0, value_blind.clone());
    // V2 circuit domain separator: DOMAIN_TOK_COMMIT = 2.
    let token_commit = poseidon_hash([pallas::Base::from(2), output.asset_id, asset_id_blind.inner()]);

    RedeemReceiptDerived { commitment, value_commit, token_commit, value }
}

/// Create a Redeem_V1 proof for the zero-value receipt commitment.
///
/// Witness order must match Redeem_V1 circuit:
///   commitment_public, value, commitment_asset_id, commitment_spend_hook,
///   commitment_user_data, commitment_blind, value_blind, asset_id_blind
///
/// Public input order: commitment, vc_x, vc_y, token_commit, value
/// value = 0 proves the receipt has no monetary value.
fn create_redeem_receipt_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &RedeemCallOutput,
    value_blind: &ScalarBlind,
    asset_id_blind: &BaseBlind,
    derived: &RedeemReceiptDerived,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, RedeemReceiptRevealed)> {
    let tx_binding = poseidon_hash([pallas::Base::from(3u64), tx_commitment, tx_nonce]);

    let public_inputs = RedeemReceiptRevealed {
        commitment: derived.commitment,
        value_commit: derived.value_commit,
        token_commit: derived.token_commit,
        value: derived.value,
        spend_hook: output.spend_hook,
        tx_binding,
        tx_nonce,
    };

    // Witness order: commitment_public, value, commitment_asset_id, commitment_spend_hook,
    //                commitment_user_data, commitment_blind, value_blind, asset_id_blind
    let prover_witnesses = vec![
        Witness::Base(Value::known(output.recipient)),
        Witness::Base(Value::known(derived.value)),
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
