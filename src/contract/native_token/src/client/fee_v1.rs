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

//! NativeToken FeeV1 Client API
//!
//! This module provides the ability to build Fee calls for network fee payment.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        note::AeadEncryptedNote,
        pasta_prelude::{Curve, CurveAffine},
        pedersen_commitment_u64, poseidon_hash, BaseBlind, Blind, FuncId, TokenId, MerkleNode,
        PublicKey, ScalarBlind, SecretKey,
    },
    error::ContractError,
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

use crate::model::{Coin, CoinAttributes, FeeParamsV1, Input, Nullifier};
use crate::client::NativeToken;

/// Fixed gas used by the fee call.
/// This is the minimum gas any fee-paying transaction will use.
pub const FEE_CALL_GAS: u64 = 42_000_000;

/// Revealed public inputs of the `Fee_V1` ZK proof
pub struct FeeRevealed {
    /// Input's Nullifier
    pub nullifier: Nullifier,
    /// Input's value commitment
    pub input_value_commit: pallas::Point,
    /// Token commitment (DRKW token = zero, ↓denominate)
    pub token_commit: pallas::Base,
    /// Merkle root for input coin
    pub merkle_root: MerkleNode,
    /// Encrypted user data for input coin
    pub input_user_data_enc: pallas::Base,
    /// Public key used to sign transaction
    pub signature_public: PublicKey,
    /// Output coin
    pub output_coin: Coin,
    /// Output value commitment
    pub output_value_commit: pallas::Point,
    /// Fee value (constrained in circuit: output_value + fee == input_value)
    pub fee: pallas::Base,
    /// Transaction commitment: hash of all call data in this transaction.
    /// Binds this proof to the specific transaction it belongs to.
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl FeeRevealed {
    /// Transform the struct into a `Vec<pallas::Base>` ready for
    /// proof verification.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let input_vc_coords = self.input_value_commit.to_affine().coordinates().expect("Value commitment cannot be the identity element");
        let output_vc_coords = self.output_value_commit.to_affine().coordinates().expect("Value commitment cannot be the identity element");
        let sigpub_coords = self.signature_public.inner().to_affine().coordinates().expect("Value commitment cannot be the identity element");

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            self.nullifier.inner(),
            *input_vc_coords.x(),
            *input_vc_coords.y(),
            self.token_commit,
            self.merkle_root.inner(),
            self.input_user_data_enc,
            *sigpub_coords.x(),
            *sigpub_coords.y(),
            self.output_coin.inner(),
            *output_vc_coords.x(),
            *output_vc_coords.y(),
            self.fee,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input for building a fee call
pub struct FeeCallInput {
    /// Value of the coin being spent
    pub value: u64,
    /// Token ID (should be DRKW_TOKEN_ID = zero)
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
    pub secret: SecretKey,
    /// Ephemeral signature secret — MUST be fresh per transaction.
    /// Never reuse the wallet secret here; doing so links all
    /// fee payments to the same on-chain signature_public.
    pub ephemeral_signature_secret: SecretKey,
    /// Transaction commitment: hash of all call data in this transaction
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Output for fee call - the "change" coin after paying fee
pub struct FeeCallOutput {
    /// Recipient public key
    pub recipient: PublicKey,
    /// Value of output coin (input_value - fee)
    pub value: u64,
    /// Spend hook for output
    pub spend_hook: pallas::Base,
    /// User data for output
    pub user_data: pallas::Base,
    /// Coin blind
    pub coin_blind: pallas::Base,
}

/// Create the `Fee_V1` ZK proof given parameters
#[allow(clippy::too_many_arguments)]
pub fn create_fee_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &FeeCallInput,
    input_value_blind: ScalarBlind,
    output: &FeeCallOutput,
    output_value_blind: ScalarBlind,
    output_spend_hook: pallas::Base,
    output_user_data: pallas::Base,
    output_coin_blind: pallas::Base,
    token_blind: BaseBlind,
    fee: u64,
    _tx_commitment: pallas::Base,
) -> Result<(Proof, FeeRevealed)> {
    // Derive public key from secret using EC (Schnorr-style)
    let public_key = PublicKey::from_secret(input.secret.clone());
    let signature_public = PublicKey::from_secret(input.ephemeral_signature_secret.clone());
    let sig_coords = signature_public.inner().to_affine().coordinates().expect("Value commitment cannot be the identity element");

    // Create input coin attributes
    let input_coin_attrs = CoinAttributes {
            version: 0,
        public_key,
        value: input.value,
        token_id: TokenId::from_base(input.token_id),
        spend_hook: FuncId::from_base(input.spend_hook),
        user_data: input.user_data,
        blind: Blind(input.coin_blind),
    };
    let input_coin = input_coin_attrs.to_coin();

    // Calculate nullifier
    let nullifier = Nullifier::new(input.secret.clone(), input_coin.inner());

    // Calculate merkle root
    let merkle_root = {
        let position: u64 = input.leaf_position.into();
        let mut current = MerkleNode::from_base(input_coin.inner());
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

    // User data encryption
    let input_user_data_enc = poseidon_hash([input.user_data, pallas::Base::zero()]);

    // Value commitments (Pedersen)
    let input_value_commit = pedersen_commitment_u64(input.value, input_value_blind);
    let output_value_commit = pedersen_commitment_u64(output.value, output_value_blind.clone());

    // Token commitment (DRKW token = zero, ↓denominate)
    let token_commit = poseidon_hash([input.token_id, token_blind.clone().inner()]);

    // Create output coin
    let output_coin_attrs = CoinAttributes {
            version: 0,
        public_key: output.recipient,
        value: output.value,
        token_id: TokenId::from_base(input.token_id), // Same token
        spend_hook: FuncId::from_base(output_spend_hook),
        user_data: output_user_data,
        blind: Blind(output_coin_blind),
    };
    let output_coin = output_coin_attrs.to_coin();

    let public_inputs = FeeRevealed {
        nullifier,
        input_value_commit,
        token_commit,
        merkle_root,
        input_user_data_enc,
        signature_public,
        output_coin,
        output_value_commit,
        fee: pallas::Base::from(fee),
        tx_binding: pallas::Base::zero(),
        tx_nonce: input.tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(*input.secret.inner())),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known({
            let mut path = input.merkle_path.clone();
            // Depth-0 tree (single coinbase leaf): empty path is correct.
            // The circuit needs at least one node — a zero node at level 0
            // is the correct sibling for a leaf with no history.
            if path.is_empty() {
                path.push(MerkleNode::from_bytes([0u8; 32])
                    .unwrap_or_else(|| MerkleNode::new(pallas::Base::zero())));
            }
            path.try_into().unwrap()
        })),
        Witness::Base(Value::known(*input.ephemeral_signature_secret.inner())),
        Witness::Base(Value::known(*sig_coords.x())),
        Witness::Base(Value::known(*sig_coords.y())),
        Witness::Base(Value::known(pallas::Base::from(input.value))),
        Witness::Scalar(Value::known(input_value_blind.clone().inner())),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.coin_blind)),
        Witness::Base(Value::known(pallas::Base::zero())),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output_spend_hook)),
        Witness::Base(Value::known(output_user_data)),
        Witness::Scalar(Value::known(output_value_blind.clone().inner())),
        Witness::Base(Value::known(output_coin_blind)),
        Witness::Base(Value::known(input.token_id)),
        Witness::Base(Value::known(token_blind.clone().inner())),
        // Fee value (constrained in circuit: output_value + fee == input_value)
        Witness::Base(Value::known(pallas::Base::from(fee))),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding computed in-circuit
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };

    Ok((proof, public_inputs))
}

/// Struct holding necessary information to build a `NativeToken::FeeV1` contract call.
pub struct FeeCallBuilder {
    /// The input coin being spent
    pub input: FeeCallInput,
    /// The output (change) coin
    pub output: FeeCallOutput,
    /// `Fee_V1` zkas circuit ZkBinary
    pub fee_zkbin: ZkBinary,
    /// Proving key for the `Fee_V1` zk circuit
    pub fee_pk: ProvingKey,
    /// Fee value being paid
    pub fee: u64,
}

/// Debris produced by building a Fee call
pub struct FeeCallDebris {
    /// The contract call parameters
    pub params: FeeParamsV1,
    /// The ZK proof
    pub proofs: Vec<Proof>,
    /// The ephemeral secret keys created for signing
    pub signature_secrets: Vec<SecretKey>,
}

impl FeeCallBuilder {
    /// Build the Fee call debris
    pub fn build(self) -> Result<FeeCallDebris> {
        if self.input.value <= self.fee {
            return Err(ContractError::Custom(1).into())
        }

        let mut proofs = vec![];
        let signature_secrets = vec![self.input.ephemeral_signature_secret.clone()];

        // Generate blinds. token_blind MUST be zero: the fee entrypoint
        // pins the native token_commit to poseidon([0, 0]) (entrypoint/mod.rs
        // fee_v1, "Token must be DRKW") with TokenId::DRKW = zero — a random
        // blind fails consensus with TokenMismatch.
        // Gated: deterministic when enable_deterministic_zk() is set
        // (MOC item 10 bug-3 — matching transfer_v1/proof.rs gating pattern).
        // Pedersen homomorphic balance requires input_blind == output_blind
        // (MoC audit: proof_of_token_balance uses blind=0 for fee, so
        //  pedersen(in_val, in_blind) == pedersen(out_val, out_blind) + pedersen(fee, 0)
        //  requires in_blind == out_blind)
        let (input_value_blind, output_coin_blind) =
            if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            (ScalarBlind::random(&mut rng), BaseBlind::random(&mut rng))
        } else {
            (ScalarBlind::random(&mut OsRng), BaseBlind::random(&mut OsRng))
        };
        let output_value_blind = input_value_blind.clone();
        let token_blind = BaseBlind::ZERO;

        // Create output value (input - fee)
        let output_value = self.input.value - self.fee;

        // Create the output with adjusted value
        let adjusted_output = FeeCallOutput {
            recipient: self.output.recipient,
            value: output_value,
            spend_hook: self.output.spend_hook,
            user_data: self.output.user_data,
            coin_blind: output_coin_blind.clone().inner(),
        };

        // Create fee proof
        let (proof, _revealed) = create_fee_proof(
            &self.fee_zkbin,
            &self.fee_pk,
            &self.input,
            input_value_blind,
            &adjusted_output,
            output_value_blind,
            self.output.spend_hook,
            self.output.user_data,
            output_coin_blind.clone().inner(),
            token_blind,
            self.fee,
            self.input.tx_commitment,
        )?;

        proofs.push(proof);

        // Build the input for params
        let input_coin_attrs = CoinAttributes {
            version: 0,
            public_key: PublicKey::from_secret(self.input.secret.clone()),
            value: self.input.value,
            token_id: TokenId::from_base(self.input.token_id),
            spend_hook: FuncId::from_base(self.input.spend_hook),
            user_data: self.input.user_data,
            blind: Blind(self.input.coin_blind),
        };
        let input_coin = input_coin_attrs.to_coin();
        let merkle_root = {
            let position: u64 = self.input.leaf_position.into();
            let mut current = MerkleNode::from_base(input_coin.inner());
            for (level, sibling) in self.input.merkle_path.iter().enumerate() {
                let level = level as u8;
                current = if position & (1 << level) == 0 {
                    MerkleNode::combine(level.into(), &current, sibling)
                } else {
                    MerkleNode::combine(level.into(), sibling, &current)
                };
            }
            current
        };

        let nullifier = Nullifier::new(self.input.secret.clone(), input_coin.inner());
        let signature_public = PublicKey::from_secret(self.input.ephemeral_signature_secret.clone());
        let input_user_data_enc = poseidon_hash([self.input.user_data, pallas::Base::zero()]);
        let input_value_commit = pedersen_commitment_u64(self.input.value, input_value_blind.clone());
        let output_value_commit = pedersen_commitment_u64(output_value, output_value_blind.clone());
        let token_commit = poseidon_hash([self.input.token_id, token_blind.clone().inner()]);
        let output_coin = CoinAttributes {
            version: 0,
            public_key: self.output.recipient,
            value: output_value,
            token_id: TokenId::from_base(self.input.token_id),
            spend_hook: FuncId::from_base(self.output.spend_hook),
            user_data: self.output.user_data,
            blind: output_coin_blind,
        }
        .to_coin();

        let params_input = Input {
            value_commit: input_value_commit,
            token_commit,
            nullifier,
            merkle_root,
            user_data_enc: input_user_data_enc,
            spend_hook: FuncId::from_base(self.input.spend_hook),
            signature_public,
        };

        // Create placeholder note for fee (fee doesn't need note encryption)
        let fee_note = NativeToken {
            value: output_value,
            token_id: self.input.token_id,
            spend_hook: self.output.spend_hook,
            user_data: self.output.user_data,
            coin_blind: output_coin_blind.clone().inner(),
            value_blind: output_value_blind.clone().inner(),
            token_blind: token_blind.clone().inner(),
            memo: vec![],
        };
        let encrypted_note = if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            AeadEncryptedNote::encrypt(&fee_note, &self.output.recipient, &mut rng)
        } else {
            AeadEncryptedNote::encrypt(&fee_note, &self.output.recipient, &mut OsRng)
        }
            .map_err(|e| ContractError::IoError(format!(
                "fee change note encryption: {:?}", e)))?;

        let output_nullifier = Nullifier::new(self.input.secret.clone(), output_coin.inner());

        let params_output = crate::model::Output {
            value_commit: output_value_commit,
            token_commit,
            coin: output_coin,
            nullifier: output_nullifier,
            note: encrypted_note,
        };

        Ok(FeeCallDebris {
            params: FeeParamsV1 {
                input: params_input,
                output: params_output,
                fee_value_blind: input_value_blind.clone().inner(),
                fee_token_blind: token_blind,
                fee: self.fee,
                tx_binding: pallas::Base::zero(),
                tx_nonce: pallas::Base::zero(),
            },
            proofs,
            signature_secrets,
        })
    }
}
