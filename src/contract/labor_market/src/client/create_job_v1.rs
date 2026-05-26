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

//! Labor Market create_job_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use rand::rngs::OsRng;

/// CreateJobV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreateJobV1PublicInputs {
    pub employer_pub_x: pallas::Base,
    pub employer_pub_y: pallas::Base,
    pub attestation_id: pallas::Base,
}

impl CreateJobV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.employer_pub_x,
            self.employer_pub_y,
            self.attestation_id,
        ]
    }
}

/// Input data for create_job proof generation
#[derive(Debug, Clone)]
pub struct CreateJobV1CallData {
    pub employer_secret: pallas::Base,
    // Public inputs
    pub employer_public: PublicKey,
    pub attestation_id: pallas::Base,
}

impl CreateJobV1CallData {
    pub fn new(employer_secret: pallas::Base, employer_public: PublicKey, attestation_id: pallas::Base) -> Self {
        Self {
            employer_secret,
            employer_public,
            attestation_id,
        }
    }

    pub fn compute_public_inputs(&self) -> CreateJobV1PublicInputs {
        let (ix, iy) = self.employer_public.xy();
        CreateJobV1PublicInputs {
            employer_pub_x: ix,
            employer_pub_y: iy,
            attestation_id: self.attestation_id,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.employer_public.xy();
        vec![
            // Must match circuit witness order:
            // employer_secret, employer_pub_x, employer_pub_y, attestation_id
            Witness::Base(Value::known(self.employer_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.attestation_id)),
        ]
    }
}

/// Create a CreateJob ZK proof
pub fn create_job_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateJobV1CallData,
) -> Result<(Proof, CreateJobV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}