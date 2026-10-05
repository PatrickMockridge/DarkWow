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

//! Bearer Bond EmergencyUnstakeV1 Client API
//!
//! Allows unstaking before maturity when coverage falls below the minimum
//! threshold (10000 bps = 100%). The holder submits a coverage report
//! proving the series is under-collateralized. Burns the stake commitment
//! (Burn_V1 proof) and creates a zero-value receipt commitment (Redeem_V1 proof).

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        pedersen_commitment_u64, poseidon_hash, BaseBlind, MerkleNode, ScalarBlind, SecretKey,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{BondInput, CommitmentAttributes, CoverageReport, EmergencyUnstakeParamsV1, Nullifier};
use super::point_coords;

/// Public inputs revealed after Burn_V1 proof (emergency unstake input side).
pub struct EmergencyUnstakeBurnRevealed {
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

impl EmergencyUnstakeBurnRevealed {
    pub fn to_vec(&self) -> Result<Vec<pallas::Base>> {
        let (vc_x, vc_y) = point_coords(self.value_commit)?;
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
pub struct EmergencyUnstakeReceiptRevealed {
    pub commitment: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub value: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl EmergencyUnstakeReceiptRevealed {
    pub fn to_vec(&self) -> Result<Vec<pallas::Base>> {
        let (vc_x, vc_y) = point_coords(self.value_commit)?;
        Ok(vec![
            self.commitment,
            vc_x,
            vc_y,
            self.token_commit,
            self.value,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ])
    }
}

/// Input for emergency unstaking a commitment.
pub struct EmergencyUnstakeCallInput {
    /// Principal value staked
    pub principal: u64,
    /// Token ID of the staking pool series
    pub asset_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Commitment blinding factor
    pub commitment_blind: pallas::Base,
    /// Block height when stake matures (ZK-committed)
    pub maturity_block: u64,
    /// Merkle tree leaf position
    pub leaf_position: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<MerkleNode>,
    /// Caller's secret key
    pub secret: pallas::Base,
    /// Ephemeral signature secret — MUST be fresh per transaction
    pub ephemeral_signature_secret: pallas::Base,
    /// Coverage report proving under-collateralization
    pub coverage_report: CoverageReport,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Output for the receipt commitment.
pub struct EmergencyUnstakeCallOutput {
    /// Redeemer's address (poseidon_hash of public key)
    pub recipient: pallas::Base,
    /// Token ID (same as unstaked commitment)
    pub asset_id: pallas::Base,
    /// Spend hook (issuer contract)
    pub spend_hook: pallas::Base,
    /// User data (emergency unstaking metadata)
    pub user_data: pallas::Base,
    /// Commitment blinding factor (fresh random)
    pub commitment_blind: pallas::Base,
}

/// Debris produced by building an EmergencyUnstake call.
pub struct EmergencyUnstakeCallDebris {
    pub params: EmergencyUnstakeParamsV1,
    pub proofs: Vec<Proof>,
}

/// Builder for `BearerBond::EmergencyUnstakeV1` contract call.
pub struct EmergencyUnstakeCallBuilder {
    pub input: EmergencyUnstakeCallInput,
    pub output: EmergencyUnstakeCallOutput,
    pub burn_zkbin: ZkBinary,
    pub burn_pk: ProvingKey,
    pub redeem_zkbin: ZkBinary,
    pub redeem_pk: ProvingKey,
}

impl EmergencyUnstakeCallBuilder {
    /// `OBL-C198`: draw the blinds, derive, and assemble the call's data — stopping **before** the
    /// proofs. The commitment covers the finished call data, so the call must exist first; `prove`
    /// is the second half.
    pub fn prepare(self) -> Result<EmergencyUnstakeCallPlan> {
        debug!(target: "contract::bearer_bond::client::emergency_unstake", "Preparing BearerBond::EmergencyUnstakeV1 contract call");

        let value_blind = ScalarBlind::random(&mut OsRng);
        let asset_id_blind = BaseBlind::random(&mut OsRng);
        let user_data_blind = BaseBlind::random(&mut OsRng);
        let burn_derived = derive_emergency_unstake_burn(
            &self.input,
            value_blind.clone(),
            asset_id_blind.clone(),
            user_data_blind.clone(),
        );

        let bond_input = BondInput {
            value_commit: burn_derived.value_commit,
            token_commit: burn_derived.token_commit,
            nullifier: burn_derived.nullifier,
            merkle_root: burn_derived.merkle_root,
            user_data_enc: burn_derived.user_data_enc,
            spend_hook: self.input.spend_hook,
            signature_public: burn_derived.signature_public,
        };

        let receipt_value_blind = ScalarBlind::random(&mut OsRng);
        let receipt_asset_id_blind = BaseBlind::random(&mut OsRng);
        let receipt_derived = derive_emergency_unstake_receipt(
            &self.output,
            receipt_value_blind.clone(),
            receipt_asset_id_blind.clone(),
        );

        let params = EmergencyUnstakeParamsV1 {
            bond_input,
            coverage_report: self.input.coverage_report.clone(),
            // The receipt's note commitment, a proof-independent derivation over the output
            // (OBL-Z15) — see the note in `unstake.rs`.
            receipt_commitment: receipt_derived.commitment,
        };
        Ok(EmergencyUnstakeCallPlan {
            burn_zkbin: self.burn_zkbin,
            burn_pk: self.burn_pk,
            redeem_zkbin: self.redeem_zkbin,
            redeem_pk: self.redeem_pk,
            input: self.input,
            output: self.output,
            value_blind,
            asset_id_blind,
            user_data_blind,
            receipt_value_blind,
            receipt_asset_id_blind,
            params,
        })
    }

    /// Build the call and prove it in one step, with the commitment the input carries.
    pub fn build(self) -> Result<EmergencyUnstakeCallDebris> {
        let (c, n) = (self.input.tx_commitment, self.input.tx_nonce);
        self.prepare()?.prove(c, n)
    }
}

/// An emergency-unstake call whose data is assembled and whose proofs are not yet made (`OBL-C198`).
pub struct EmergencyUnstakeCallPlan {
    burn_zkbin: ZkBinary,
    burn_pk: ProvingKey,
    redeem_zkbin: ZkBinary,
    redeem_pk: ProvingKey,
    input: EmergencyUnstakeCallInput,
    output: EmergencyUnstakeCallOutput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
    receipt_value_blind: ScalarBlind,
    receipt_asset_id_blind: BaseBlind,
    params: EmergencyUnstakeParamsV1,
}

impl EmergencyUnstakeCallPlan {
    /// The contract's call parameters.
    pub fn params(&self) -> EmergencyUnstakeParamsV1 { self.params.clone() }

    /// Prove the burn and the receipt, binding each to `tx_commitment`.
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<EmergencyUnstakeCallDebris> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        let (burn_proof, _burn_revealed) = create_emergency_unstake_burn_proof(
            &self.burn_zkbin,
            &self.burn_pk,
            &self.input,
            self.value_blind,
            self.asset_id_blind,
            self.user_data_blind,
        )?;
        let (receipt_proof, _receipt_revealed) = create_emergency_unstake_receipt_proof(
            &self.redeem_zkbin,
            &self.redeem_pk,
            &self.output,
            self.receipt_value_blind,
            self.receipt_asset_id_blind,
            tx_commitment,
            tx_nonce,
        )?;
        Ok(EmergencyUnstakeCallDebris { params: self.params, proofs: vec![burn_proof, receipt_proof] })
    }
}

/// The commitment-independent values an emergency-unstake `Burn_V1` proof reveals (`OBL-C198`).
pub struct EmergencyUnstakeBurnDerived {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub signature_public: pallas::Base,
}

/// Derive, do not prove — see `EmergencyUnstakeBurnDerived`.
pub fn derive_emergency_unstake_burn(
    input: &EmergencyUnstakeCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> EmergencyUnstakeBurnDerived {
    let public_key = poseidon_hash([pallas::Base::from(7), input.secret]);

    let commitment = CommitmentAttributes {
        public_key,
        value: input.principal,
        asset_id: input.asset_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.commitment_blind,
        maturity_block: input.maturity_block,
    }
    .to_commitment();

    let nullifier = Nullifier::new(SecretKey::from_base(input.secret), commitment);

    let merkle_root = {
        let position: u64 = input.leaf_position;
        let mut current = MerkleNode::from_base(commitment);
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

    EmergencyUnstakeBurnDerived {
        nullifier,
        value_commit: pedersen_commitment_u64(input.principal, value_blind),
        token_commit: poseidon_hash([pallas::Base::from(2), input.asset_id, asset_id_blind.inner()]),
        merkle_root,
        user_data_enc: poseidon_hash([pallas::Base::from(6), input.user_data, user_data_blind.inner()]),
        signature_public: poseidon_hash([pallas::Base::from(7), input.ephemeral_signature_secret]),
    }
}

/// The commitment-independent values an emergency-unstake `Redeem_V2` receipt proof reveals
/// (`OBL-C198`).
pub struct EmergencyUnstakeReceiptDerived {
    pub commitment: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
}

/// Derive, do not prove — see `EmergencyUnstakeReceiptDerived`.
pub fn derive_emergency_unstake_receipt(
    output: &EmergencyUnstakeCallOutput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
) -> EmergencyUnstakeReceiptDerived {
    let attrs = CommitmentAttributes {
        public_key: output.recipient,
        value: 0,
        asset_id: output.asset_id,
        spend_hook: output.spend_hook,
        user_data: output.user_data,
        blind: output.commitment_blind,
        maturity_block: 0,
    };
    EmergencyUnstakeReceiptDerived {
        commitment: attrs.to_commitment(),
        value_commit: pedersen_commitment_u64(0, value_blind),
        token_commit: poseidon_hash([pallas::Base::from(2), output.asset_id, asset_id_blind.inner()]),
    }
}

fn create_emergency_unstake_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &EmergencyUnstakeCallInput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, EmergencyUnstakeBurnRevealed)> {
    let derived = derive_emergency_unstake_burn(input, value_blind.clone(), asset_id_blind.clone(), user_data_blind.clone());

    let public_inputs = EmergencyUnstakeBurnRevealed {
        nullifier: derived.nullifier,
        value_commit: derived.value_commit,
        token_commit: derived.token_commit,
        merkle_root: derived.merkle_root,
        user_data_enc: derived.user_data_enc,
        spend_hook: input.spend_hook,
        signature_public: derived.signature_public,
        tx_binding: poseidon_hash([
            dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING,
            input.tx_commitment,
            input.tx_nonce,
        ]),
        tx_nonce: input.tx_nonce,
    };

    #[expect(clippy::unwrap_used, reason = "guarded by tree structure")]
    let prover_witnesses = vec![
        Witness::Base(Value::known(input.secret)),
        Witness::Base(Value::known(pallas::Base::from(input.principal))),
        Witness::Base(Value::known(input.asset_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.commitment_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(asset_id_blind.inner())),
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
        Witness::Base(Value::known(poseidon_hash([
            dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING,
            input.tx_commitment,
            input.tx_nonce,
        ]))), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut OsRng)?;

    Ok((proof, public_inputs))
}

fn create_emergency_unstake_receipt_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &EmergencyUnstakeCallOutput,
    value_blind: ScalarBlind,
    asset_id_blind: BaseBlind,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, EmergencyUnstakeReceiptRevealed)> {
    let value = pallas::Base::zero();
    let derived = derive_emergency_unstake_receipt(output, value_blind.clone(), asset_id_blind.clone());

    // `OBL-C198`: the pair is bound here, from the transaction commitment the caller set.
    let tx_binding = poseidon_hash([dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING, tx_commitment, tx_nonce]);
    let public_inputs = EmergencyUnstakeReceiptRevealed {
        commitment: derived.commitment,
        value_commit: derived.value_commit,
        token_commit: derived.token_commit,
        value,
        spend_hook: output.spend_hook,
        tx_binding,
        tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(output.recipient)),
        Witness::Base(Value::known(value)),
        Witness::Base(Value::known(output.asset_id)),
        Witness::Base(Value::known(output.spend_hook)),
        Witness::Base(Value::known(output.user_data)),
        Witness::Base(Value::known(output.commitment_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(asset_id_blind.inner())),
        Witness::Base(Value::known(tx_commitment)), // tx_commitment
        Witness::Base(Value::known(tx_nonce)), // tx_nonce
        Witness::Base(Value::known(tx_binding)), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec()?, &mut OsRng)?;

    Ok((proof, public_inputs))
}
