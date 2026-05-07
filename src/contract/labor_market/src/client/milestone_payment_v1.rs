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

//! Labor Market milestone_payment_v1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// MilestonePaymentV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct MilestonePaymentV1PublicInputs {
    pub job_id: pallas::Base,
    pub employer_pub_x: pallas::Base,
    pub employer_pub_y: pallas::Base,
    pub milestone_payment_amount: pallas::Base,
    pub spent_nullifier: pallas::Base,
}

impl MilestonePaymentV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.job_id,
            self.employer_pub_x,
            self.employer_pub_y,
            self.milestone_payment_amount,
            self.spent_nullifier,
        ]
    }
}

/// Input data for milestone_payment proof generation
#[derive(Debug, Clone)]
pub struct MilestonePaymentV1CallData {
    pub job_id: pallas::Base,
    pub milestone_payment_amount: pallas::Base,
    pub employer_secret: pallas::Base,
    // Public inputs
    pub employer_public: PublicKey,
    pub last_milestone_block: pallas::Base,
    pub current_block: pallas::Base,
    pub deadline_block: pallas::Base,
}

impl MilestonePaymentV1CallData {
    pub fn new(
        job_id: pallas::Base,
        milestone_payment_amount: pallas::Base,
        employer_secret: pallas::Base,
        employer_public: PublicKey,
        last_milestone_block: pallas::Base,
        current_block: pallas::Base,
        deadline_block: pallas::Base,
    ) -> Self {
        Self {
            job_id,
            milestone_payment_amount,
            employer_secret,
            employer_public,
            last_milestone_block,
            current_block,
            deadline_block,
        }
    }

    /// Compute nullifier from job_id and employer_secret
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.job_id, self.employer_secret])
    }

    pub fn compute_public_inputs(&self) -> MilestonePaymentV1PublicInputs {
        let (ix, iy) = self.employer_public.xy();
        MilestonePaymentV1PublicInputs {
            job_id: self.job_id,
            employer_pub_x: ix,
            employer_pub_y: iy,
            milestone_payment_amount: self.milestone_payment_amount,
            spent_nullifier: self.compute_nullifier(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.employer_public.xy();
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.job_id)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.milestone_payment_amount)),
            Witness::Base(Value::known(self.compute_nullifier())),
            // Private inputs
            Witness::Base(Value::known(self.milestone_payment_amount)),
            Witness::Base(Value::known(self.employer_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.last_milestone_block)),
            Witness::Base(Value::known(self.current_block)),
            Witness::Base(Value::known(self.deadline_block)),
        ]
    }
}

/// Create a MilestonePayment ZK proof
pub fn milestone_payment_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &MilestonePaymentV1CallData,
) -> Result<(Proof, MilestonePaymentV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}