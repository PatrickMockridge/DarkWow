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

//! Labor Market refund_v1 ZK proof generation

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

/// RefundV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct RefundV1PublicInputs {
    pub job_id: pallas::Base,
    pub employer_pub_x: pallas::Base,
    pub employer_pub_y: pallas::Base,
    pub milestone_count: pallas::Base,
    pub completed_payment: pallas::Base,
    pub refund_amount: pallas::Base,
    pub spent_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RefundV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.job_id,
            self.employer_pub_x,
            self.employer_pub_y,
            self.milestone_count,
            self.completed_payment,
            self.refund_amount,
            self.spent_nullifier,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for refund proof generation
#[derive(Debug, Clone)]
pub struct RefundV1CallData {
    pub job_id: pallas::Base,
    pub employer_secret: pallas::Base,
    pub milestone_count: pallas::Base,
    pub completed_payment: pallas::Base,
    pub refund_amount: pallas::Base,
    pub deadline_block: pallas::Base,
    pub current_block: pallas::Base,
    pub total_payment: pallas::Base,
    // Public inputs
    pub employer_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RefundV1CallData {
    pub fn new(
        job_id: pallas::Base,
        employer_secret: pallas::Base,
        milestone_count: pallas::Base,
        completed_payment: pallas::Base,
        refund_amount: pallas::Base,
        deadline_block: pallas::Base,
        current_block: pallas::Base,
        total_payment: pallas::Base,
        employer_public: PublicKey,
    ) -> Self {
        Self {
            job_id,
            employer_secret,
            milestone_count,
            completed_payment,
            refund_amount,
            deadline_block,
            current_block,
            total_payment,
            employer_public,
            tx_commitment: pallas::Base::zero(),
        }
    }

    /// Compute nullifier from job_id and employer_secret
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.job_id, self.employer_secret])
    }

    pub fn compute_public_inputs(&self) -> RefundV1PublicInputs {
        let (ix, iy) = self.employer_public.xy();
        RefundV1PublicInputs {
            job_id: self.job_id,
            employer_pub_x: ix,
            employer_pub_y: iy,
            milestone_count: self.milestone_count,
            completed_payment: self.completed_payment,
            refund_amount: self.refund_amount,
            spent_nullifier: self.compute_nullifier(),
            tx_binding: poseidon_hash([self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.employer_public.xy();
        vec![
            // Must match circuit witness order:
            // job_id, employer_secret, employer_pub_x, employer_pub_y,
            // milestone_count, completed_payment, refund_amount,
            // deadline_block, current_block, total_payment
            // (spent_nullifier is computed by the circuit via poseidon_hash)
            Witness::Base(Value::known(self.job_id)),
            Witness::Base(Value::known(self.employer_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.milestone_count)),
            Witness::Base(Value::known(self.completed_payment)),
            Witness::Base(Value::known(self.refund_amount)),
            Witness::Base(Value::known(self.deadline_block)),
            Witness::Base(Value::known(self.current_block)),
            Witness::Base(Value::known(self.total_payment)),
        ]
    }
}

/// Create a Refund ZK proof
pub fn refund_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RefundV1CallData,
) -> Result<(Proof, RefundV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}