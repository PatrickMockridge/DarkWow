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

//! DAO-Escrow ResolveDispute ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// ResolveDisputeV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct ResolveDisputeV1PublicInputs {
    pub capability_id: pallas::Base,
    pub dao_escrow_bulla: pallas::Base,
    pub dispute_id: pallas::Base,
    pub attestation_root: pallas::Base,
    pub resolution_commit: pallas::Base,
    pub dispute_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ResolveDisputeV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.capability_id,
            self.dao_escrow_bulla,
            self.dispute_id,
            self.attestation_root,
            self.resolution_commit,
            self.dispute_nullifier,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for ResolveDispute proof generation
#[derive(Debug, Clone)]
pub struct ResolveDisputeV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub capability_id: pallas::Base,
    pub dao_escrow_bulla: pallas::Base,
    pub dispute_id: pallas::Base,
    pub capability_secret: pallas::Base,
    pub arbitrator_secret: pallas::Base,
    pub attestation_count: u64,
    pub threshold: u64,
    pub resolution_result: bool,
    pub payout_amount: u64,
    pub recipient_pub_x: pallas::Base,
    pub recipient_pub_y: pallas::Base,
    pub attestation_root: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ResolveDisputeV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        capability_id: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        dispute_id: pallas::Base,
        capability_secret: pallas::Base,
        arbitrator_secret: pallas::Base,
        attestation_count: u64,
        threshold: u64,
        resolution_result: bool,
        payout_amount: u64,
        payout_recipient: PublicKey,
        attestation_root: pallas::Base,
    ) -> Self {
        let (rx, ry) = payout_recipient.xy();
        Self {
            nullifier_k,
            capability_id,
            dao_escrow_bulla,
            dispute_id,
            capability_secret,
            arbitrator_secret,
            attestation_count,
            threshold,
            resolution_result,
            payout_amount,
            recipient_pub_x: rx,
            recipient_pub_y: ry,
            attestation_root,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> ResolveDisputeV1PublicInputs {
        let dispute_nullifier = poseidon_hash([
            self.capability_secret,
            self.dispute_id,
            self.dao_escrow_bulla,
        ]);

        let resolution_commit = poseidon_hash([
            self.dispute_id,
            pallas::Base::from(self.resolution_result as u64),
            pallas::Base::from(self.payout_amount),
            self.recipient_pub_x,
            self.recipient_pub_y,
            self.attestation_root,
        ]);

        ResolveDisputeV1PublicInputs {
            capability_id: self.capability_id,
            dao_escrow_bulla: self.dao_escrow_bulla,
            dispute_id: self.dispute_id,
            attestation_root: self.attestation_root,
            resolution_commit,
            dispute_nullifier,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.capability_id)),
            Witness::Base(Value::known(self.dao_escrow_bulla)),
            Witness::Base(Value::known(self.dispute_id)),
            Witness::Base(Value::known(self.capability_secret)),
            Witness::Base(Value::known(self.arbitrator_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.attestation_count))),
            Witness::Base(Value::known(pallas::Base::from(self.threshold))),
            Witness::Base(Value::known(pallas::Base::from(self.resolution_result as u64))),
            Witness::Base(Value::known(pallas::Base::from(self.payout_amount))),
            Witness::Base(Value::known(self.recipient_pub_x)),
            Witness::Base(Value::known(self.recipient_pub_y)),
            Witness::Base(Value::known(self.attestation_root)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a ResolveDispute ZK proof
pub fn resolve_dispute_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ResolveDisputeV1CallData,
) -> Result<(Proof, ResolveDisputeV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
