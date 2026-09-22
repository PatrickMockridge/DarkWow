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
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// AcceptJobWithCapabilityV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct AcceptJobWithCapabilityV1PublicInputs {
    pub spent_nullifier: pallas::Base,
    pub job_id: pallas::Base,
    pub worker_pub_x: pallas::Base,
    pub worker_pub_y: pallas::Base,
    pub required_capability_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AcceptJobWithCapabilityV1PublicInputs {
    /// Order must match `accept_job_with_capability.zk`'s `constrain_instance` sequence exactly:
    ///   spent_nullifier, job_id, worker_pub_x, worker_pub_y, capability_id, tx_binding, tx_nonce
    /// `spent_nullifier` led the circuit and was published nowhere, so the vector was six against
    /// seven — and the seven were misaligned from position 1 anyway (register OBL-C78).
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.spent_nullifier,
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
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AcceptJobWithCapabilityV1CallData {
    /// `capability_nullifier` and `capability_predicate_result` used to be taken here and passed
    /// straight through to `to_witnesses`, where `accept_job_with_capability.zk` declares no such
    /// witnesses — two of the three extra entries (register OBL-C78). The capability is bound as
    /// the instance `capability_id`, which is what `required_capability_id` supplies.
    pub fn new(
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
        required_capability_id: pallas::Base,
    ) -> Self {
        Self {
            worker_secret,
            worker_public,
            job_id,
            required_capability_id,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// `AcceptJobWithCapabilityV2` derives `spent_nullifier = poseidon_hash(1, 8, job_id,
    /// worker_secret)` and constrains it at instance 1. Domain 1 = `NULLIFIER`, tag 8 = the
    /// circuit's `ACCEPT_WITH_CAP_TAG`. Note it does **not** depend on the capability: the
    /// capability is bound as instance 5 (`capability_id`) instead (register OBL-C78).
    pub fn compute_spent_nullifier(&self) -> pallas::Base {
        poseidon_hash([
            pallas::Base::from(1u64),
            pallas::Base::from(8u64),
            self.job_id,
            self.worker_secret,
        ])
    }

    /// `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)`, domain 3, instance 6.
    pub fn compute_tx_binding(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce])
    }

    pub fn compute_public_inputs(&self) -> AcceptJobWithCapabilityV1PublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.worker_public.xy().expect("pk not identity");
        AcceptJobWithCapabilityV1PublicInputs {
            spent_nullifier: self.compute_spent_nullifier(),
            job_id: self.job_id,
            worker_pub_x: ix,
            worker_pub_y: iy,
            required_capability_id: self.required_capability_id,
            tx_binding: self.compute_tx_binding(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.worker_public.xy().expect("pk not identity");
        // Must match `accept_job_with_capability.zk`'s `witness` block exactly:
        //   job_id, worker_secret, worker_pub_x, worker_pub_y, capability_id, tx_commitment,
        //   tx_nonce, tx_binding
        // `capability_nullifier` and `capability_predicate_result` were supplied here and are
        // declared nowhere — the vector was ten entries against eight, and the first four were in
        // the wrong order too (register OBL-C78). `spent_nullifier` is derived in-circuit, so it
        // is an instance, not a witness.
        vec![
            Witness::Base(Value::known(self.job_id)),
            Witness::Base(Value::known(self.worker_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.required_capability_id)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.compute_tx_binding())), // tx_binding
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