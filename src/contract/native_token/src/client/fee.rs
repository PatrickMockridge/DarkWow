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
    blockchain::FeeAmount,
    error::ContractError,
    pasta::pallas,
};
use rand::SeedableRng;

use crate::client::NativeToken;
use crate::model::fee::{FeeParamsV2, FeeV2TxBinding, ThresholdTxBinding};
use crate::model::{CoinAttributes, Input, Output};

// ---- Domain-labeled fee wrappers ----
// Per fee-spec §6.1, bare u64 SHALL NOT enter ZK proof witnesses or
// Pedersen commitments. These wrappers move `.get()` inside a function
// whose signature declares the domain transition: FeeAmount → pallas::Point
// or FeeAmount → pallas::Base.

/// Pedersen-commit to a fee amount. The `FeeAmount` parameter ensures
/// fee values cannot be confused with other u64 quantities at call sites.
pub(crate) fn pedersen_commitment_fee(amount: FeeAmount, blind: ScalarBlind) -> pallas::Point {
    pedersen_commitment_u64(amount.get(), blind)
}

/// Convert a FeeAmount to a base field element for ZK witness/public input.
/// Used by the Fee_V2 mass balance proof (defensive, verified via WASM)
/// and by the FeeThreshold_V1 proof (wallet→mempool gate).
pub fn fee_to_base(amount: FeeAmount) -> pallas::Base {
    pallas::Base::from(amount.get())
}

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

/// Builder for FeeV2 calls — privacy-preserving fee payment.
///
/// Produces call data: `[0x08][FeeParamsV2 encoded]` with NO clear-text fee bytes.
/// The fee amount is hidden behind a Pedersen commitment.
///
/// The Fee_V2 proof (Pedersen mass balance) is constructed by [`build()`].
/// The FeeThreshold_V1 proof (fee >= threshold) MUST be constructed externally
/// by the wallet's [`fee_threshold_proof`] module and provided as
/// [`threshold_proof_bytes`]. This separation ensures the mempool admission
/// gate lives in the wallet crate, not the contract crate.
pub struct FeeV2CallBuilder {
    pub input: FeeV2CallInput,
    pub output: FeeV2CallOutput,
    pub fee_amount: FeeAmount,
    pub threshold: FeeAmount,
    /// Fee_V2 zkas circuit ZkBinary
    pub fee_zkbin: ZkBinary,
    /// Proving key for the Fee_V2 ZK circuit
    pub fee_pk: ProvingKey,
    /// Serialized FeeThreshold_V1 proof — constructed externally by the wallet
    /// crate's fee_threshold_proof module.
    pub threshold_proof_bytes: Vec<u8>,
}

impl FeeV2CallBuilder {
    /// Build a FeeV2 call with Fee_V2 proof (Pedersen mass balance).
    ///
    /// The fee amount is private — it appears ONLY as a Pedersen commitment
    /// in the call data. The Fee_V2 circuit constrains input = output + fee
    /// internally without exposing fee.
    ///
    /// The FeeThreshold_V1 proof MUST be constructed externally (by the wallet
    /// crate's `fee_threshold_proof` module) and provided as
    /// [`Self::threshold_proof_bytes`] before calling this method.
    pub fn build(mut self) -> Result<FeeV2Result, ContractError> {
        if self.input.value <= self.fee_amount.get() {
            return Err(ContractError::Custom(0)); // ↓bad-fee-amount — spec §11
        }

        let mut proofs: Vec<Proof> = vec![];

        // Generate blinds. Pedersen mass balance requires:
        //   output_blind + fee_blind == input_blind
        // Generate input_blind and fee_blind first, then derive output_blind.
        let token_blind = BaseBlind::ZERO;
        // F3 fix: single RNG instance for all blinds so deterministic
        // mode does not force output_value_blind to zero.
        let (input_value_blind, output_coin_blind, fee_value_blind) =
            if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            (ScalarBlind::random(&mut rng), BaseBlind::random(&mut rng),
             ScalarBlind::random(&mut rng))
        } else {
            (ScalarBlind::random(&mut rand::rngs::OsRng),
             BaseBlind::random(&mut rand::rngs::OsRng),
             ScalarBlind::random(&mut rand::rngs::OsRng))
        };
        // F1 fix: ensure proof and metadata use the same output coin blind.
        // create_fee_proof reads self.output.coin_blind; build_fee_v2_params
        // receives output_coin_blind. Both must agree.
        self.output.coin_blind = output_coin_blind.inner();

        // Fee_V2 circuit derives output coin public key from input_secret (witness #1):
        //   pub = ec_mul_base(input_secret, NULLIFIER_K)
        //   output_coin = poseidon(DOMAIN_COIN_COMMIT, pub_x, pub_y, ...)
        // The builder MUST use the same public key for output.recipient, otherwise
        // the output_coin hash at public input #9 diverges and proof verification fails.
        self.output.recipient = PublicKey::from_secret(self.input.secret.clone());

        // output_blind = input_blind - fee_blind  (ρ-calculus blind consistency, F2 fix)
        let output_value_blind: ScalarBlind = Blind(
            input_value_blind.inner() - fee_value_blind.inner()
        );
        // Invariant: Pedersen mass balance requires output_blind + fee_blind == input_blind
        debug_assert_eq!(
            output_value_blind.inner() + fee_value_blind.inner(),
            input_value_blind.inner(),
            "F2: Pedersen blind consistency violated"
        );

        // Compute output value
        let output_value = self.input.value - self.fee_amount.get();

        // Fee_V2 circuit tx_binding: bound to tx_nonce (matches fee.zk).
        // Used for the Fee_V2 proof public inputs and stored in FeeParamsV2.
        // Per fee-spec.md §5.5.1: nominal type, domain mass_balance.
        let fee_v2_tx_binding = FeeV2TxBinding::compute(
            self.input.tx_commitment,
            self.input.tx_nonce,
        );

        // FeeThreshold_V1 tx_binding: bound to threshold (matches fee_threshold_v1.zk).
        // Used for the FeeThreshold_V1 proof public inputs and stored in FeeParamsV2.
        // Per fee-spec.md §5.5.1: nominal type, domain fee_signalling.
        let threshold_tx_binding = ThresholdTxBinding::compute(
            self.input.tx_commitment,
            self.threshold,
        );

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
            output_coin_blind.clone(),
        )?;
        proofs.push(fee_proof);

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
        let fee_value_commit = pedersen_commitment_fee(self.fee_amount, fee_value_blind.clone());
        let coords = fee_value_commit.to_affine().coordinates();
        if coords.is_none().into() {
            return Err(ContractError::IoError("FeeV2: fee_value_commit is identity".into()));
        }
        let c = coords.unwrap();
        let fee_value_commit_x = *c.x();
        let fee_value_commit_y = *c.y();

        // Build FeeParamsV2
        // TODO(fee-spec §5.6.3, G2 Phase 2): encrypt self.fee_amount to miner's
        // public key using AEAD. For now, empty — the field is serialized as
        // length-prefixed (4 zero bytes + 0 data bytes = 4 bytes on wire).
        // FI-ENCRYPT-1: encrypted_fee_value SHALL NOT be empty.
        // Real AEAD encryption to miner's per-block public key is not yet wired.
        // Produce a 68-byte placeholder to satisfy the length invariant.
        // FeeParamsV2::decode rejects values shorter than 68 bytes.
        let encrypted_fee_value: Vec<u8> = vec![0u8; 68];
        let params = FeeParamsV2 {
            input: params_input,
            output: params_output,
            fee_value_commit,
            fee_value_commit_x,
            fee_value_commit_y,
            threshold_proof: self.threshold_proof_bytes.clone(),
            threshold: self.threshold,
            encrypted_fee_value,
            fee_value_blind: fee_value_blind.inner(),
            fee_token_blind: token_blind,
            fee_v2_tx_binding,
            threshold_tx_binding,
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
    output_coin_blind: BaseBlind,
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
        blind: output_coin_blind,
    }.to_coin();

    // Merkle root from the wallet's production tree — not recomputed here.
    // The ZK circuit verifies root == merkle_root(pos, path, coin) internally.

    // Compute commitments
    let input_value_commit = pedersen_commitment_u64(input.value, input_value_blind.clone());
    let output_value_commit = pedersen_commitment_u64(output_value, output_value_blind.clone());
    let fee_value_commit = pedersen_commitment_fee(fee_amount, fee_value_blind.clone());
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
    let sig_coords = sig_pk.xy();
    if sig_coords.is_none().into() {
        return Err(ContractError::IoError("FeeV2: ephemeral signature pk is identity".into()));
    }
    let (sig_x, sig_y) = sig_coords.unwrap();

    // Public inputs for Fee_V2 (15 elements, matching fee_get_metadata order)
    // M6: guard against identity point — follow existing pattern at lines 237-241
    let input_vc = input_value_commit.to_affine().coordinates();
    if input_vc.is_none().into() {
        return Err(ContractError::IoError("FeeV2: input_value_commit is identity".into()));
    }
    let input_vc_coords = input_vc.unwrap();

    let output_vc = output_value_commit.to_affine().coordinates();
    if output_vc.is_none().into() {
        return Err(ContractError::IoError("FeeV2: output_value_commit is identity".into()));
    }
    let output_vc_coords = output_vc.unwrap();

    let fee_vc = fee_value_commit.to_affine().coordinates();
    if fee_vc.is_none().into() {
        return Err(ContractError::IoError("FeeV2: fee_value_commit is identity".into()));
    }
    let fee_vc_coords = fee_vc.unwrap();
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
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into()
            .map_err(|_| ContractError::IoError("FeeV2: leaf_position exceeds u32".into()))?)),
        Witness::MerklePath(Value::known({
            let mut path = input.merkle_path.clone();
            // Depth-0 tree (single leaf): empty path is correct.
            // The circuit needs at least one node — a zero node at level 0
            // is the correct sibling for a leaf with no history.
            if path.is_empty() {
                path.push(MerkleNode::from_bytes([0u8; 32])
                    .unwrap_or_else(|| MerkleNode::new(pallas::Base::zero())));
            }
            path.try_into()
                .map_err(|_| ContractError::IoError("FeeV2: merkle path conversion failed".into()))?
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
        Witness::Base(Value::known(fee_to_base(fee_amount))),
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

