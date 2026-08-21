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
//! coin (destroying monetary value) and creates a zero-value receipt coin —
//! cryptographic proof that redemption occurred with the issuer.
//!
//! The receipt coin is non-transferable (spend_hook = issuer contract) and serves
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

fn point_to_coords(pt: pallas::Point) -> (pallas::Base, pallas::Base) {
    let affine = pt.to_affine();
    let coords = affine.coordinates().unwrap();
    (*coords.x(), *coords.y())
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

/// Public inputs revealed after Redeem_V1 receipt proof.
/// Order must match Redeem_V1 circuit:
/// coin, value_commit_x, value_commit_y, token_commit, coin_value, spend_hook
pub struct RedeemReceiptRevealed {
    pub commitment: CapCommitment,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub coin_value: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RedeemReceiptRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_to_coords(self.value_commit);
        vec![
            self.commitment.inner(),
            vc_x,
            vc_y,
            self.token_commit,
            self.coin_value,
            self.tx_binding,
            self.tx_nonce,
            self.spend_hook,
        ]
    }
}

// ============================================================================
// BUILDER INPUTS
// ============================================================================

/// Input for redeeming a coin.
pub struct RedeemCallInput {
    /// Value of the coin being redeemed
    pub value: u64,
    /// Token ID
    pub asset_id: pallas::Base,
    /// Spend hook (issuer contract ID)
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
    /// Ephemeral signature secret — MUST be fresh per redemption.
    pub ephemeral_signature_secret: pallas::Base,
}

/// Output for the receipt coin.
pub struct RedeemCallOutput {
    /// Recipient address (poseidon_hash of public key X coord)
    pub recipient: pallas::Base,
    /// Recipient's public key for AEAD note encryption
    pub recipient_pub: PublicKey,
    /// Token ID (same as redeemed coin)
    pub asset_id: pallas::Base,
    /// Spend hook (issuer contract — makes receipt non-transferable)
    pub spend_hook: pallas::Base,
    /// User data (redemption metadata)
    pub user_data: pallas::Base,
    /// Coin blind (fresh random per redemption)
    pub coin_blind: pallas::Base,
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
    /// Coin being redeemed
    pub input: RedeemCallInput,
    /// Receipt coin output
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
    pub fn build(self) -> Result<RedeemCallDebris> {
        debug!(target: "contract::promissory_note::client::redeem", "Building PromissoryNote::RedeemV1 contract call");

        let mut proofs = vec![];

        // Build burn proof for the input coin being redeemed
        let (value_blind, asset_id_blind, user_data_blind) =
            if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            (ScalarBlind::random(&mut rng), BaseBlind::random(&mut rng),
             BaseBlind::random(&mut rng))
        } else {
            (ScalarBlind::random(&mut OsRng), BaseBlind::random(&mut OsRng),
             BaseBlind::random(&mut OsRng))
        };

        let (burn_proof, burn_revealed) = create_redeem_burn_proof(
            &self.burn_zkbin,
            &self.burn_pk,
            &self.input,
            value_blind,
            asset_id_blind,
            user_data_blind,
            self.tx_commitment,
            self.tx_nonce,
        )?;

        proofs.push(burn_proof);

        let input = Input {
            value_commit: burn_revealed.value_commit,
            token_commit: burn_revealed.token_commit,
            nullifier: burn_revealed.nullifier,
            merkle_root: burn_revealed.merkle_root,
            user_data_enc: burn_revealed.user_data_enc,
            spend_hook: FuncId::from_base(self.input.spend_hook),
            signature_public: burn_revealed.signature_public,
        };

        // Build Redeem_V1 proof for the zero-value receipt coin
        let (receipt_value_blind, receipt_asset_id_blind) =
            if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            (ScalarBlind::random(&mut rng), BaseBlind::random(&mut rng))
        } else {
            (ScalarBlind::random(&mut OsRng), BaseBlind::random(&mut OsRng))
        };

        let (output_proof, output_revealed) = create_redeem_receipt_proof(
            &self.redeem_zkbin,
            &self.redeem_pk,
            &self.output,
            receipt_value_blind.clone(),
            receipt_asset_id_blind.clone(),
            self.tx_commitment,
            self.tx_nonce,
        )?;

        proofs.push(output_proof);

        // Build note for the receipt so the redeemer can discover it via trial-decryption
        let note = PromissoryNote {
            value: 0,
            asset_id: self.output.asset_id,
            spend_hook: self.output.spend_hook,
            user_data: self.output.user_data,
            coin_blind: self.output.coin_blind,
            value_blind: receipt_value_blind.inner(),
            token_blind: receipt_asset_id_blind.inner(),
            memo: vec![],
            commitment: output_revealed.commitment.inner(),
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
            value_commit: output_revealed.value_commit,
            token_commit: output_revealed.token_commit,
            commitment: output_revealed.commitment,
            note: encrypted_note,
            spend_hook: FuncId::from_base(self.output.spend_hook),
        };

        Ok(RedeemCallDebris {
            params: RedeemParamsV1 { input, output,
                tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]), tx_nonce: self.tx_nonce },
            proofs,
        })
    }
}

// ============================================================================
// PROOF CREATION
// ============================================================================

/// Create a burn proof for the input coin being redeemed.
/// Reuses the existing Burn_V1 circuit.
fn create_redeem_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RedeemCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, RedeemRevokeRevealed)> {
    // V2 circuit domain separator: DOMAIN_SIGNATURE_SECRET = 7.
    let public_key = poseidon_hash([pallas::Base::from(7), input.secret]);

    let commitment = CapAttrs {
        public_key,
        value: input.value,
        asset_id: AssetId::from_base(input.asset_id),
        spend_hook: FuncId::from_base(input.spend_hook),
        user_data: input.user_data,
        blind: Blind(input.coin_blind),
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
    // V2 circuit derives signature_secret = H(7, coin_secret, nullifier) and
    // signature_public = H(7, signature_secret) — matches revoke.rs / revoke.zk.
    let signature_secret = poseidon_hash([pallas::Base::from(7), input.secret, nullifier.inner()]);
    let signature_public = poseidon_hash([pallas::Base::from(7), signature_secret]);
    let tx_binding = poseidon_hash([pallas::Base::from(3u64), tx_commitment, tx_nonce]);

    let public_inputs = RedeemRevokeRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        user_data_enc,
        spend_hook: input.spend_hook,
        signature_public,
        tx_binding,
        tx_nonce,
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
        Witness::Base(Value::known(input.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(asset_id_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(leaf_position)),
        Witness::MerklePath(Value::known(merkle_path)),
        Witness::Base(Value::known(signature_secret)),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_nonce)),
        Witness::Base(Value::known(tx_binding)), // tx_binding (shadowed, recomputed in-circuit)
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

/// Create a Redeem_V1 proof for the zero-value receipt coin.
///
/// Witness order must match Redeem_V1 circuit:
///   coin_public, coin_value, coin_asset_id, coin_spend_hook,
///   coin_user_data, coin_blind, value_blind, asset_id_blind
///
/// Public input order: coin, vc_x, vc_y, token_commit, coin_value
/// coin_value = 0 proves the receipt has no monetary value.
fn create_redeem_receipt_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &RedeemCallOutput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, RedeemReceiptRevealed)> {
    let coin_value = pallas::Base::zero();
    let attrs = CapAttrs {
        public_key: output.recipient,
        value: 0,
        asset_id: AssetId::from_base(output.asset_id),
        spend_hook: FuncId::from_base(output.spend_hook),
        user_data: output.user_data,
        blind: Blind(output.coin_blind),
    };
    let commitment = attrs.to_commitment();

    let value_commit = pedersen_commitment_u64(0, value_blind.clone());
    // V2 circuit domain separator: DOMAIN_TOK_COMMIT = 2.
    let token_commit = poseidon_hash([pallas::Base::from(2), output.asset_id, asset_id_blind.inner()]);

    let tx_binding = poseidon_hash([pallas::Base::from(3u64), tx_commitment, tx_nonce]);

    let public_inputs = RedeemReceiptRevealed {
        commitment,
        value_commit,
        token_commit,
        coin_value,
        spend_hook: output.spend_hook,
        tx_binding,
        tx_nonce,
    };

    // Witness order: coin_public, coin_value, coin_asset_id, coin_spend_hook,
    //                coin_user_data, coin_blind, value_blind, asset_id_blind
    let prover_witnesses = vec![
        Witness::Base(Value::known(output.recipient)),
        Witness::Base(Value::known(coin_value)),
        Witness::Base(Value::known(output.asset_id)),
        Witness::Base(Value::known(output.spend_hook)),
        Witness::Base(Value::known(output.user_data)),
        Witness::Base(Value::known(output.coin_blind)),
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
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
