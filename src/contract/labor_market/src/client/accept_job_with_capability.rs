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

//! Labor Market accept_job_with_capability_v1 ZK proof generation

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
use rand::SeedableRng;

/// AcceptJobWithCapabilityV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct AcceptJobWithCapabilityV1PublicInputs {
    pub job_id: pallas::Base,
    pub worker_pub_x: pallas::Base,
    pub worker_pub_y: pallas::Base,
    pub required_capability_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AcceptJobWithCapabilityV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.job_id,
            self.worker_pub_x,
            self.worker_pub_y,
            self.required_capability_id,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for accept_job_with_capability proof generation
#[derive(Debug, Clone)]
pub struct AcceptJobWithCapabilityV1CallData {
    pub worker_secret: pallas::Base,
    // Public inputs
    pub worker_public: PublicKey,
    pub job_id: pallas::Base,
    pub required_capability_id: pallas::Base,
    // Capability verification data
    pub capability_nullifier: pallas::Base,
    pub capability_predicate_result: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AcceptJobWithCapabilityV1CallData {
    pub fn new(
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
        required_capability_id: pallas::Base,
        capability_nullifier: pallas::Base,
        capability_predicate_result: pallas::Base,
    ) -> Self {
        Self {
            worker_secret,
            worker_public,
            job_id,
            required_capability_id,
            capability_nullifier,
            capability_predicate_result,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> AcceptJobWithCapabilityV1PublicInputs {
        let (ix, iy) = self.worker_public.xy().expect("pk not identity");
        AcceptJobWithCapabilityV1PublicInputs {
            job_id: self.job_id,
            worker_pub_x: ix,
            worker_pub_y: iy,
            required_capability_id: self.required_capability_id,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.worker_public.xy().expect("pk not identity");
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.job_id)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.required_capability_id)),
            // Capability proof data (witnesses)
            Witness::Base(Value::known(self.capability_nullifier)),
            Witness::Base(Value::known(self.capability_predicate_result)),
            // Private inputs
            Witness::Base(Value::known(self.worker_secret)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create an AcceptJobWithCapability ZK proof
pub fn accept_job_with_capability_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &AcceptJobWithCapabilityV1CallData,
) -> Result<(Proof, AcceptJobWithCapabilityV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}