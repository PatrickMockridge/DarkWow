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
        pedersen_commitment_u64, poseidon_hash, BaseBlind, MerkleNode, PublicKey, ScalarBlind, Blind, FuncId, TokenId,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use super::PromissoryNote;
use crate::model::{AeadEncryptedNote, Coin, CoinAttributes, Input, Nullifier, Output, RedeemParamsV1};

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
pub struct RedeemBurnRevealed {
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

impl RedeemBurnRevealed {
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
    pub coin: Coin,
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
            self.coin.inner(),
            vc_x,
            vc_y,
            self.token_commit,
            self.coin_value,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
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
    pub token_id: pallas::Base,
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
    pub token_id: pallas::Base,
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
        let value_blind = ScalarBlind::random(&mut OsRng);
        let token_id_blind = BaseBlind::random(&mut OsRng);
        let user_data_blind = BaseBlind::random(&mut OsRng);

        let (burn_proof, burn_revealed) = create_redeem_burn_proof(
            &self.burn_zkbin,
            &self.burn_pk,
            &self.input,
            value_blind,
            token_id_blind,
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
            spend_hook: FuncId::from(self.input.spend_hook),
            signature_public: burn_revealed.signature_public,
        };

        // Build Redeem_V1 proof for the zero-value receipt coin
        let receipt_value_blind = ScalarBlind::random(&mut OsRng);
        let receipt_token_id_blind = BaseBlind::random(&mut OsRng);

        let (output_proof, output_revealed) = create_redeem_receipt_proof(
            &self.redeem_zkbin,
            &self.redeem_pk,
            &self.output,
            receipt_value_blind,
            receipt_token_id_blind,
            self.tx_commitment,
            self.tx_nonce,
        )?;

        proofs.push(output_proof);

        // Build note for the receipt so the redeemer can discover it via trial-decryption
        let note = PromissoryNote {
            value: 0,
            token_id: self.output.token_id,
            spend_hook: self.output.spend_hook,
            user_data: self.output.user_data,
            coin_blind: self.output.coin_blind,
            value_blind: receipt_value_blind.inner(),
            token_blind: receipt_token_id_blind.inner(),
            memo: vec![],
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &self.output.recipient_pub, &mut OsRng)
            .map_err(|e| crate::error::ContractError::Custom(match e {
                crate::error::ContractError::Custom(n) => n,
                _ => u32::MAX,
            }))?;

        let output = Output {
            value_commit: output_revealed.value_commit,
            token_commit: output_revealed.token_commit,
            coin: output_revealed.coin,
            note: encrypted_note,
            spend_hook: FuncId::from(self.output.spend_hook),
        };

        Ok(RedeemCallDebris {
            params: RedeemParamsV1 { input, output,
                tx_binding: pallas::Base::zero(), tx_nonce: self.input.tx_nonce },
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
    token_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, RedeemBurnRevealed)> {
    let public_key = poseidon_hash([input.secret]);

    let coin = CoinAttributes {
        public_key,
        value: input.value,
        token_id: TokenId(input.token_id),
        spend_hook: FuncId::from(input.spend_hook),
        user_data: input.user_data,
        blind: Blind(input.coin_blind),
    }
    .to_coin();

    let nullifier = Nullifier::new(input.secret, coin.inner());

    let merkle_root = {
        let position: u64 = input.leaf_position.into();
        let mut current = MerkleNode::from(coin.inner());
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

    let value_commit = pedersen_commitment_u64(input.value, value_blind);
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);
    let user_data_enc = poseidon_hash([input.user_data, user_data_blind.inner()]);
    let signature_public = poseidon_hash([input.ephemeral_signature_secret]);

    let public_inputs = RedeemBurnRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        user_data_enc,
        spend_hook: input.spend_hook,
        signature_public,
        tx_binding: pallas::Base::zero(),
        tx_nonce,
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
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_nonce)),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding computed in-circuit
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}

/// Create a Redeem_V1 proof for the zero-value receipt coin.
///
/// Witness order must match Redeem_V1 circuit:
///   coin_public, coin_value, coin_token_id, coin_spend_hook,
///   coin_user_data, coin_blind, value_blind, token_id_blind
///
/// Public input order: coin, vc_x, vc_y, token_commit, coin_value
/// coin_value = 0 proves the receipt has no monetary value.
fn create_redeem_receipt_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &RedeemCallOutput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, RedeemReceiptRevealed)> {
    let coin_value = pallas::Base::zero();
    let attrs = CoinAttributes {
        public_key: output.recipient,
        value: 0,
        token_id: TokenId(output.token_id),
        spend_hook: FuncId::from(output.spend_hook),
        user_data: output.user_data,
        blind: Blind(output.coin_blind),
    };
    let coin = attrs.to_coin();

    let value_commit = pedersen_commitment_u64(0, value_blind);
    let token_commit = poseidon_hash([output.token_id, token_id_blind.inner()]);

    let public_inputs = RedeemReceiptRevealed {
        coin,
        value_commit,
        token_commit,
        coin_value,
        spend_hook: output.spend_hook,
        tx_binding: pallas::Base::zero(),
        tx_nonce,
    };

    // Witness order: coin_public, coin_value, coin_token_id, coin_spend_hook,
    //                coin_user_data, coin_blind, value_blind, token_id_blind
    let prover_witnesses = vec![
        Witness::Base(Value::known(output.recipient)),
        Witness::Base(Value::known(coin_value)),
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
