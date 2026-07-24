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

//! Bearer Bond TransferStakeV1 Client API
//!
//! Transfer a stake position to a new holder. The new stake coin preserves
//! `last_claim_block` from the old coin, so unclaimed profit distributions
//! travel with the coin.
//!
//! Uses Burn_V1 for inputs (proving ownership) and BlindOutput_V1 for outputs
//! (creating new stake coins for recipients).

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        pedersen_commitment_u64, poseidon_hash, BaseBlind, ContractId, MerkleNode, ScalarBlind,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{BondCoin, BondInput, CoinAttributes, Nullifier, TransferStakeParamsV1};
use super::point_coords;

// ============================================================================
// REVEALED PUBLIC INPUTS
// ============================================================================

/// Public inputs revealed after Burn_V1 proof (transfer input side).
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
        let (vc_x, vc_y) = point_coords(self.value_commit);
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

/// Public inputs revealed after BlindOutput_V1 proof (transfer output side).
/// Order must match BlindOutput_V1 circuit:
/// coin, value_commit_x, value_commit_y, token_commit, spend_hook
pub struct TransferBlindOutputRevealed {
    pub coin: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl TransferBlindOutputRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_coords(self.value_commit);
        vec![
            self.coin,
            vc_x,
            vc_y,
            self.token_commit,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

// ============================================================================
// BUILDER INPUTS
// ============================================================================

/// Input coin for transfer — the stake being transferred.
#[derive(Clone)]
pub struct TransferStakeCallInput {
    /// Principal value staked
    pub principal: u64,
    /// Token ID of the staking pool series
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blinding factor
    pub coin_blind: pallas::Base,
    /// Block height of last profit claim (preserved on output)
    pub last_claim_block: u64,
    /// Block height when stake matures
    pub maturity_block: u64,
    /// Issuer contract ID
    pub issuer_contract: ContractId,
    /// Merkle tree leaf position
    pub leaf_position: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<MerkleNode>,
    /// Caller's secret key
    pub secret: pallas::Base,
    /// Ephemeral signature secret (Schnorr) — MUST be fresh per transaction
    pub ephemeral_signature_secret: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Output coin for transfer — the new stake coin for recipient.
#[derive(Clone)]
pub struct TransferStakeCallOutput {
    /// Recipient address (poseidon_hash of public key X coord)
    pub recipient: pallas::Base,
    /// Principal value (same as input)
    pub principal: u64,
    /// Token ID (same as input)
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blinding factor (fresh random per output)
    pub coin_blind: pallas::Base,
    /// Last claim block (inherited from input — unclaimed profits travel with coin)
    pub last_claim_block: u64,
    /// Maturity block (same as input)
    pub maturity_block: u64,
    /// Issuer contract ID (same as input)
    pub issuer_contract: ContractId,
}

// ============================================================================
// DEBRIS
// ============================================================================

/// Debris produced by building a TransferStake call.
pub struct TransferStakeCallDebris {
    /// The contract call parameters
    pub params: TransferStakeParamsV1,
    /// The ZK proofs (burn proofs first, then blind output proofs)
    pub proofs: Vec<Proof>,
    /// Private note data for each output (recipient needs these to spend)
    pub output_notes: Vec<super::BearerBondNote>,
}

// ============================================================================
// BUILDER
// ============================================================================

/// Builder for `BearerBond::TransferStakeV1` contract call.
pub struct TransferStakeCallBuilder {
    /// Anonymous inputs being spent
    pub inputs: Vec<TransferStakeCallInput>,
    /// Anonymous outputs being created
    pub outputs: Vec<TransferStakeCallOutput>,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for Burn_V1
    pub burn_pk: ProvingKey,
    /// `BlindOutput_V1` zkas circuit ZkBinary
    pub blind_output_zkbin: ZkBinary,
    /// Proving key for BlindOutput_V1
    pub blind_output_pk: ProvingKey,
}

impl TransferStakeCallBuilder {
    /// Build the TransferStake call debris.
    pub fn build(self) -> Result<TransferStakeCallDebris> {
        debug!(target: "contract::bearer_bond::client::transfer", "Building BearerBond::TransferStakeV1 contract call");

        if self.inputs.is_empty() {
            return Err(dwow_sdk::error::ContractError::Custom(
                crate::error::BearerBondError::MissingInputs.code(),
            )
            .into());
        }
        if self.outputs.is_empty() {
            return Err(dwow_sdk::error::ContractError::Custom(
                crate::error::BearerBondError::MissingOutputs.code(),
            )
            .into());
        }

        let mut proofs = vec![];
        let mut inputs = vec![];
        let mut outputs = vec![];
        let mut output_notes = vec![];

        // Build Burn_V1 proofs for inputs
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

            inputs.push(BondInput {
                value_commit: revealed.value_commit,
                token_commit: revealed.token_commit,
                nullifier: revealed.nullifier,
                merkle_root: revealed.merkle_root,
                user_data_enc: revealed.user_data_enc,
                spend_hook: input.spend_hook,
                signature_public: revealed.signature_public,
            });
        }

        // Build BlindOutput_V1 proofs for outputs
        for output in self.outputs.clone() {
            let value_blind = ScalarBlind::random(&mut OsRng);
            let token_id_blind = BaseBlind::random(&mut OsRng);

            let (blind_output_proof, revealed) = create_transfer_blind_output_proof(
                &self.blind_output_zkbin,
                &self.blind_output_pk,
                &output,
                value_blind,
                token_id_blind,
            )?;

            proofs.push(blind_output_proof);

            outputs.push(BondCoin {
                value_commit: revealed.value_commit,
                token_commit: revealed.token_commit,
                nullifier: Nullifier::from_base(pallas::Base::zero()),
                merkle_root: MerkleNode::from_base(pallas::Base::zero()),
                user_data_enc: pallas::Base::zero(),
                spend_hook: output.spend_hook,
                signature_public: output.recipient,
                last_claim_block: output.last_claim_block,
                maturity_block: output.maturity_block,
                issuer_contract: output.issuer_contract,
            });

            // Build the note for the recipient so they can reconstruct coin attributes
            output_notes.push(super::BearerBondNote {
                principal: output.principal,
                token_id: output.token_id,
                spend_hook: output.spend_hook,
                user_data: output.user_data,
                coin_blind: output.coin_blind,
                value_blind: value_blind.inner(),
                token_blind: token_id_blind.inner(),
                last_claim_block: output.last_claim_block,
                maturity_block: output.maturity_block,
                issuer_contract: output.issuer_contract,
                interest_rate_bps: 0,
            });
        }

        Ok(TransferStakeCallDebris {
            params: TransferStakeParamsV1 { inputs, outputs },
            proofs,
            output_notes,
        })
    }
}

// ============================================================================
// PROOF CREATION
// ============================================================================

/// Create a Burn_V1 proof for transferring a stake coin.
///
/// Witness order must match Burn_V1 circuit:
/// secret, value, token_id, spend_hook, user_data, coin_blind,
/// value_blind, token_id_blind, user_data_blind, leaf_position,
/// merkle_path, ephemeral_signature_secret
fn create_transfer_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransferStakeCallInput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, TransferBurnRevealed)> {
    let public_key = poseidon_hash([input.secret]);

    let coin = CoinAttributes {
        public_key,
        value: input.principal,
        token_id: input.token_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.coin_blind,
        maturity_block: input.maturity_block,
    }
    .to_coin();

    let nullifier = Nullifier::new(input.secret, coin);

    let merkle_root = {
        let position: u64 = input.leaf_position;
        let mut current = MerkleNode::from_base(coin);
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

    let value_commit = pedersen_commitment_u64(input.principal, value_blind);
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);
    let user_data_enc = poseidon_hash([input.user_data, user_data_blind.inner()]);
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
        Witness::Base(Value::known(pallas::Base::from(input.principal))),
        Witness::Base(Value::known(input.token_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(
            u64::from(input.leaf_position).try_into().unwrap(),
        )),
        Witness::MerklePath(Value::known(
            input.merkle_path.clone().try_into().unwrap(),
        )),
        Witness::Base(Value::known(input.ephemeral_signature_secret)),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}

/// Create a BlindOutput_V1 proof for the new stake coin.
///
/// Witness order must match BlindOutput_V1 circuit:
/// coin_public, coin_value, coin_token_id, coin_spend_hook,
/// coin_user_data, coin_blind, value_blind, token_id_blind
fn create_transfer_blind_output_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &TransferStakeCallOutput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
) -> Result<(Proof, TransferBlindOutputRevealed)> {
    let attrs = CoinAttributes {
        public_key: output.recipient,
        value: output.principal,
        token_id: output.token_id,
        spend_hook: output.spend_hook,
        user_data: output.user_data,
        blind: output.coin_blind,
        maturity_block: output.maturity_block,
    };
    let coin = attrs.to_coin();

    let value_commit = pedersen_commitment_u64(output.principal, value_blind);
    let token_commit = poseidon_hash([output.token_id, token_id_blind.inner()]);

    let public_inputs = TransferBlindOutputRevealed {
        coin,
        value_commit,
        token_commit,
        spend_hook: output.spend_hook,
        tx_binding: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(output.recipient)),
        Witness::Base(Value::known(pallas::Base::from(output.principal))),
        Witness::Base(Value::known(output.token_id)),
        Witness::Base(Value::known(output.spend_hook)),
        Witness::Base(Value::known(output.user_data)),
        Witness::Base(Value::known(output.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_commitment
        Witness::Base(Value::known(pallas::Base::zero())), // tx_nonce
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
