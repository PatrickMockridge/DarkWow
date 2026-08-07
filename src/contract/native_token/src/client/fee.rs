/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! NativeToken FeeV2 Client API
//!
//! This module provides the ability to build privacy-preserving FeeV2 calls.
//! FeeV2 hides the fee amount behind a Pedersen commitment and includes a
//! FeeThreshold_V1 proof that the fee meets a threshold without revealing it.
//!
//! Spec: fee-spec.md §5.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result as CoreResult,
};
use dwow_sdk::crypto::{
    constants::{
        DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, DRK_POSEIDON_DOMAIN_TX_BINDING,
        DRK_POSEIDON_DOMAIN_USER_DATA_ENC,
    },
    note::AeadEncryptedNote,
    pasta_prelude::{Curve, CurveAffine},
    pedersen_commitment_u64, poseidon_hash,
    BaseBlind, Blind, FuncId, MerkleNode, Nullifier, PublicKey, ScalarBlind, SecretKey, TokenId,
};
use dwow_sdk::{
    bridgetree::Hashable,
    error::ContractError,
    pasta::pallas,
};
use rand::{RngCore, SeedableRng};

use crate::client::NativeToken;
use crate::model::fee::FeeParamsV2;
use crate::client::zkbins::{
    NATIVE_TOKEN_CONTRACT_ZKAS_FEE_THRESHOLD_V1_BIN,
};
use crate::model::{CoinAttributes, Input, Output};

/// Input parameters for building a FeeV2 call.
pub struct FeeV2CallInput {
    pub value: u64,
    pub token_id: pallas::Base,
    pub spend_hook: pallas::Base,
    pub user_data: pallas::Base,
    pub coin_blind: pallas::Base,
    pub leaf_position: u64,
    pub merkle_path: Vec<MerkleNode>,
    /// Merkle root from the wallet's production tree (tree.root(0)).
    /// SHALL NOT be recomputed manually in the proof builder.
    /// The ZK circuit verifies root == merkle_root(pos, path, coin).
    pub merkle_root: MerkleNode,
    pub secret: SecretKey,
    pub ephemeral_signature_secret: SecretKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Output specification for a FeeV2 call.
pub struct FeeV2CallOutput {
    pub recipient: PublicKey,
    pub value: u64,
    pub spend_hook: pallas::Base,
    pub user_data: pallas::Base,
    pub coin_blind: pallas::Base,
}

/// Result of building a FeeV2 call.
pub struct FeeV2Result {
    pub call_data: Vec<u8>,
    pub params: FeeParamsV2,
    pub proofs: Vec<Proof>,
}

/// Builder for FeeV2 calls — privacy-preserving fee with threshold proof.
///
/// Produces call data: `[0x08][FeeParamsV2 encoded]` with NO clear-text fee bytes.
/// The fee amount is hidden behind a Pedersen commitment.
pub struct FeeV2CallBuilder {
    pub input: FeeV2CallInput,
    pub output: FeeV2CallOutput,
    pub fee_amount: FeeAmount,
    pub threshold: FeeAmount,
    /// Fee_V2 zkas circuit ZkBinary
    pub fee_zkbin: ZkBinary,
    /// Proving key for the Fee_V2 ZK circuit
    pub fee_pk: ProvingKey,
    /// FeeThreshold_V1 zkas circuit ZkBinary
    pub threshold_zkbin: ZkBinary,
    /// Proving key for the FeeThreshold_V1 ZK circuit
    pub threshold_pk: ProvingKey,
}

impl FeeV2CallBuilder {
    /// Build a FeeV2 call with dual ZK proofs (Fee_V2 + FeeThreshold_V1).
    ///
    /// The fee amount is private — it appears ONLY as a Pedersen commitment
    /// in the call data. The Fee_V2 circuit constrains input = output + fee
    /// internally without exposing fee. The FeeThreshold_V1 circuit proves
    /// fee >= threshold.
    pub fn build(self) -> Result<FeeV2Result, ContractError> {
        if self.input.value <= self.fee_amount.get() {
            return Err(ContractError::Custom(1)); // ↓bad-fee-amount
        }

        let mut proofs: Vec<Proof> = vec![];

        // Generate blinds
        let (input_value_blind, output_coin_blind) =
            if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            (ScalarBlind::random(&mut rng), BaseBlind::random(&mut rng))
        } else {
            (ScalarBlind::random(&mut rand::rngs::OsRng),
             BaseBlind::random(&mut rand::rngs::OsRng))
        };
        let output_value_blind = input_value_blind.clone();
        let token_blind = BaseBlind::ZERO;

        // Generate fee_value_blind
        let fee_value_blind = if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            ScalarBlind::random(&mut rng)
        } else {
            ScalarBlind::random(&mut rand::rngs::OsRng)
        };

        // Compute output value
        let output_value = self.input.value - self.fee_amount.get();

        // Fee_V2 circuit tx_binding: bound to tx_nonce (matches fee.zk).
        // Used for the Fee_V2 proof public inputs and stored in FeeParamsV2.
        let fee_v2_tx_binding = poseidon_hash([
            DRK_POSEIDON_DOMAIN_TX_BINDING,
            self.input.tx_commitment,
            self.input.tx_nonce,
        ]);

        // FeeThreshold_V1 tx_binding: bound to threshold (matches fee_threshold_v1.zk).
        // Used for the FeeThreshold_V1 proof only.
        let threshold_tx_binding = poseidon_hash([
            DRK_POSEIDON_DOMAIN_TX_BINDING,
            self.input.tx_commitment,
            pallas::Base::from(self.threshold.get()),
        ]);

        // Build Fee_V2 proof using pre-built proving key
        let (fee_proof, _revealed) = create_fee_proof(
            &self.fee_zkbin,
            &self.fee_pk,
            &self.input,
            input_value_blind.clone(),
            &self.output,
            output_value_blind.clone(),
            token_blind.clone(),
            self.fee_amount,
            fee_value_blind.clone(),
            self.input.tx_commitment,
        )?;
        proofs.push(fee_proof);

        // Build FeeThreshold_V1 proof using pre-built proving key
        let threshold_proof = create_fee_threshold_proof(
            &self.threshold_zkbin,
            &self.threshold_pk,
            self.fee_amount,
            self.threshold,
            self.input.tx_commitment,
            threshold_tx_binding,
        )?;
        proofs.push(threshold_proof.clone());

        // Serialize threshold proof for embedding in params
        let mut proof_bytes = vec![];
        dwow_serial::Encodable::encode(&threshold_proof, &mut proof_bytes)
            .map_err(|e| ContractError::IoError(format!("threshold proof encode: {:?}", e)))?;

        // Build Input/Output params
        let (params_input, params_output) = build_fee_v2_params(
            &self.input,
            &self.output,
            input_value_blind,
            output_value_blind,
            output_coin_blind,
            token_blind.clone(),
            output_value,
        )?;

        // Compute fee_value_commit and extract coordinates
        let fee_value_commit = pedersen_commitment_u64(self.fee_amount.get(), fee_value_blind.clone());
        let coords = fee_value_commit.to_affine().coordinates();
        if coords.is_none().into() {
            return Err(ContractError::IoError("FeeV2: fee_value_commit is identity".into()));
        }
        let c = coords.unwrap();
        let fee_value_commit_x = *c.x();
        let fee_value_commit_y = *c.y();

        // Build FeeParamsV2
        let params = FeeParamsV2 {
            input: params_input,
            output: params_output,
            fee_value_commit,
            fee_value_commit_x,
            fee_value_commit_y,
            threshold_proof: proof_bytes,
            threshold: self.threshold,
            fee_value_blind: fee_value_blind.inner(),
            fee_token_blind: token_blind,
            tx_binding: fee_v2_tx_binding,
            tx_nonce: self.input.tx_nonce,
        };

        // Serialize call data: [0x08][FeeParamsV2 encoded]
        let encoded_params = params.encode();
        let mut call_data = Vec::with_capacity(1 + encoded_params.len());
        call_data.push(0x08u8);
        call_data.extend_from_slice(&encoded_params);

        Ok(FeeV2Result { call_data, params, proofs })
    }
}

/// Build Input and Output parameters for a FeeV2 call.
fn build_fee_v2_params(
    input: &FeeV2CallInput,
    output: &FeeV2CallOutput,
    input_value_blind: ScalarBlind,
    output_value_blind: ScalarBlind,
    output_coin_blind: BaseBlind,
    token_blind: BaseBlind,
    output_value: u64,
) -> Result<(Input, Output), ContractError> {
    // Build input coin
    let input_coin_attrs = CoinAttributes {
        version: 0,
        public_key: PublicKey::from_secret(input.secret.clone()),
        value: input.value,
        token_id: TokenId::from_base(input.token_id),
        spend_hook: FuncId::from_base(input.spend_hook),
        user_data: input.user_data,
        blind: Blind(input.coin_blind),
    };
    let input_coin = input_coin_attrs.to_coin();

    // Merkle root from the wallet's production tree — not recomputed here.

    let nullifier = Nullifier::new(input.secret.clone(), input_coin.inner());
    let signature_public = PublicKey::from_secret(input.ephemeral_signature_secret.clone());
    let input_user_data_enc = poseidon_hash([
        DRK_POSEIDON_DOMAIN_USER_DATA_ENC, input.user_data, pallas::Base::zero(),
    ]);
    let input_value_commit = pedersen_commitment_u64(input.value, input_value_blind.clone());
    let output_value_commit = pedersen_commitment_u64(output_value, output_value_blind.clone());
    let token_commit_val = poseidon_hash([
        DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, input.token_id, token_blind.inner(),
    ]);

    // Build output coin
    let output_coin = CoinAttributes {
        version: 0,
        public_key: output.recipient,
        value: output_value,
        token_id: TokenId::from_base(input.token_id),
        spend_hook: FuncId::from_base(output.spend_hook),
        user_data: output.user_data,
        blind: output_coin_blind.clone(),
    }.to_coin();

    // Encrypt output note
    let fee_note = NativeToken {
        value: output_value,
        token_id: input.token_id,
        spend_hook: output.spend_hook,
        user_data: output.user_data,
        coin_blind: output_coin_blind.inner(),
        value_blind: output_value_blind.inner(),
        token_blind: token_blind.inner(),
        memo: vec![],
    };
    let encrypted_note = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        AeadEncryptedNote::encrypt(&fee_note, &output.recipient, &mut rng)
    } else {
        AeadEncryptedNote::encrypt(&fee_note, &output.recipient, &mut rand::rngs::OsRng)
    }.map_err(|e| ContractError::IoError(format!("FeeV2 note encrypt: {:?}", e)))?;

    let output_nullifier = Nullifier::new(input.secret.clone(), output_coin.inner());

    Ok((
        Input {
            value_commit: input_value_commit,
            token_commit: token_commit_val,
            nullifier,
            merkle_root: input.merkle_root,
            user_data_enc: input_user_data_enc,
            spend_hook: FuncId::from_base(input.spend_hook),
            signature_public,
        },
        Output {
            value_commit: output_value_commit,
            token_commit: token_commit_val,
            coin: output_coin,
            nullifier: output_nullifier,
            note: encrypted_note,
        },
    ))
}

/// Create a Fee_V2 ZK proof (value conservation with hidden fee).
fn create_fee_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &FeeV2CallInput,
    input_value_blind: ScalarBlind,
    output: &FeeV2CallOutput,
    output_value_blind: ScalarBlind,
    token_blind: BaseBlind,
    fee_amount: FeeAmount,
    fee_value_blind: ScalarBlind,
    tx_commitment: pallas::Base,
) -> Result<(Proof, Vec<pallas::Base>), ContractError> {
    let output_value = input.value - fee_amount.get();

    // Build input coin
    let input_coin_attrs = CoinAttributes {
        version: 0,
        public_key: PublicKey::from_secret(input.secret.clone()),
        value: input.value,
        token_id: TokenId::from_base(input.token_id),
        spend_hook: FuncId::from_base(input.spend_hook),
        user_data: input.user_data,
        blind: Blind(input.coin_blind),
    };
    let input_coin = input_coin_attrs.to_coin();
    let nullifier = Nullifier::new(input.secret.clone(), input_coin.inner());

    // Build output coin
    let output_coin = CoinAttributes {
        version: 0,
        public_key: output.recipient,
        value: output_value,
        token_id: TokenId::from_base(input.token_id),
        spend_hook: FuncId::from_base(output.spend_hook),
        user_data: output.user_data,
        blind: Blind(output.coin_blind),
    }.to_coin();

    // Merkle root from the wallet's production tree — not recomputed here.
    // The ZK circuit verifies root == merkle_root(pos, path, coin) internally.

    // Compute commitments
    let input_value_commit = pedersen_commitment_u64(input.value, input_value_blind.clone());
    let output_value_commit = pedersen_commitment_u64(output_value, output_value_blind.clone());
    let fee_value_commit = pedersen_commitment_u64(fee_amount.get(), fee_value_blind.clone());
    let token_commit = poseidon_hash([
        DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, input.token_id, token_blind.inner(),
    ]);
    let user_data_enc = poseidon_hash([
        DRK_POSEIDON_DOMAIN_USER_DATA_ENC, input.user_data, pallas::Base::zero(),
    ]);
    let tx_binding = poseidon_hash([
        DRK_POSEIDON_DOMAIN_TX_BINDING, tx_commitment, input.tx_nonce,
    ]);
    let sig_pk = PublicKey::from_secret(input.ephemeral_signature_secret.clone());
    let (sig_x, sig_y) = sig_pk.xy().expect("pk not identity");

    // Public inputs for Fee_V2 (15 elements, matching fee_get_metadata order)
    let input_vc_coords = input_value_commit.to_affine().coordinates().unwrap();
    let output_vc_coords = output_value_commit.to_affine().coordinates().unwrap();
    let fee_vc_coords = fee_value_commit.to_affine().coordinates().unwrap();
    let public_inputs = vec![
        nullifier.inner(),                // 1
        *input_vc_coords.x(),             // 2
        *input_vc_coords.y(),             // 3
        token_commit,                     // 4
        input.merkle_root.inner(),        // 5 — from wallet's production tree
        user_data_enc,                    // 6
        sig_x,                            // 7
        sig_y,                            // 8
        output_coin.inner(),              // 9
        *output_vc_coords.x(),            // 10
        *output_vc_coords.y(),            // 11
        *fee_vc_coords.x(),               // 12: fee_value_commit x
        *fee_vc_coords.y(),               // 13: fee_value_commit y
        tx_binding,                       // 14
        input.tx_nonce,                   // 15
    ];

    // Build witnesses (matching circuit witness order in fee.zk "Fee_V2")
    let prover_witnesses = vec![
        Witness::Base(Value::known(*input.secret.inner())),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known({
            let mut path = input.merkle_path.clone();
            // Depth-0 tree (single leaf): empty path is correct.
            // The circuit needs at least one node — a zero node at level 0
            // is the correct sibling for a leaf with no history.
            if path.is_empty() {
                path.push(MerkleNode::from_bytes([0u8; 32])
                    .unwrap_or_else(|| MerkleNode::new(pallas::Base::zero())));
            }
            path.try_into().unwrap()
        })),
        Witness::Base(Value::known(*input.ephemeral_signature_secret.inner())),
        Witness::Base(Value::known(sig_x)),
        Witness::Base(Value::known(sig_y)),
        Witness::Base(Value::known(pallas::Base::from(input.value))),
        Witness::Scalar(Value::known(input_value_blind.inner())),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.coin_blind)),
        Witness::Base(Value::known(pallas::Base::zero())), // input_user_data_blind
        Witness::Base(Value::known(pallas::Base::from(output_value))),
        Witness::Base(Value::known(output.spend_hook)),
        Witness::Base(Value::known(output.user_data)),
        Witness::Scalar(Value::known(output_value_blind.inner())),
        Witness::Base(Value::known(output.coin_blind)),
        Witness::Base(Value::known(input.token_id)),
        Witness::Base(Value::known(token_blind.inner())),
        Witness::Base(Value::known(pallas::Base::from(fee_amount.get()))),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        // tx_binding — MUST be the computed value, not zero.
        // Circuit constrains: tx_binding = poseidon(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)
        Witness::Base(Value::known(tx_binding)),
        Witness::Scalar(Value::known(fee_value_blind.inner())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        Proof::create(&pk, &[circuit], &public_inputs, &mut rng)
            .map_err(|e| ContractError::IoError(format!("FeeV2 Fee_V2 proof: {:?}", e)))?
    } else {
        Proof::create(&pk, &[circuit], &public_inputs, &mut rand::rngs::OsRng)
            .map_err(|e| ContractError::IoError(format!("FeeV2 Fee_V2 proof: {:?}", e)))?
    };

    Ok((proof, public_inputs))
}

/// Create a FeeThreshold_V1 ZK proof (fee >= threshold without revealing fee).
fn create_fee_threshold_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    fee_amount: FeeAmount,
    threshold: FeeAmount,
    tx_commitment: pallas::Base,
    tx_binding: pallas::Base,
) -> Result<Proof, ContractError> {

    // FeeThreshold_V1 witnesses (4): fee, threshold, tx_commitment, tx_binding
    let witnesses: Vec<Witness> = vec![
        Witness::Base(Value::known(pallas::Base::from(fee_amount.get()))),
        Witness::Base(Value::known(pallas::Base::from(threshold.get()))),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_binding)),
    ];

    // Public inputs (2): threshold, tx_binding
    let public_inputs = vec![
        pallas::Base::from(threshold.get()),
        tx_binding,
    ];

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let rng: Box<dyn RngCore + Send> = if crate::deterministic_zk_enabled() {
        Box::new(rand::rngs::StdRng::seed_from_u64(43))
    } else {
        Box::new(rand::rngs::OsRng)
    };
    let proof = Proof::create(&pk, &[circuit], &public_inputs, rng)
        .map_err(|e| ContractError::IoError(format!("FeeV2 threshold proof: {:?}", e)))?;

    Ok(proof)
}
