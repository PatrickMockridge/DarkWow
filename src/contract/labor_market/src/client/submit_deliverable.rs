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

//! Labor Market submit_deliverable_v1 ZK proof generation

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

/// SubmitDeliverableV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SubmitDeliverableV1PublicInputs {
    pub spent_nullifier: pallas::Base,
    pub job_id: pallas::Base,
    pub worker_pub_x: pallas::Base,
    pub worker_pub_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SubmitDeliverableV1PublicInputs {
    /// Order must match `submit_deliverable.zk`'s `constrain_instance` sequence exactly:
    ///   spent_nullifier, job_id, worker_pub_x, worker_pub_y, tx_binding, tx_nonce
    /// `spent_nullifier` sat at position 5 where the circuit leads with it, and `claim_id` was
    /// published although the circuit constrains no such instance — a seven-element vector against
    /// a six-instance circuit (register OBL-C78). `claim_id` remains in the params, where exec
    /// needs it to validate the attestation child; being needed by exec is not being a public
    /// input. Print the circuit before changing this again.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.spent_nullifier,
            self.job_id,
            self.worker_pub_x,
            self.worker_pub_y,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for submit_deliverable proof generation
#[derive(Debug, Clone)]
pub struct SubmitDeliverableV1CallData {
    pub worker_secret: pallas::Base,
    // Public inputs
    pub worker_public: PublicKey,
    pub job_id: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SubmitDeliverableV1CallData {
    /// `claim_id`, `deadline_block` and `current_block` used to be taken here and threaded
    /// straight into `to_witnesses` — where the circuit declares no such witnesses, so they were
    /// three of the four extra entries that made the witness vector the wrong length (register
    /// OBL-C78). The deadline is not a witness at all: `SubmitDeliverableV2` constrains no
    /// deadline, and the params carry none, so no deadline check exists on this path. `claim_id`
    /// stays in the params, where exec needs it to validate the attestation child — needed by exec
    /// is not the same as being a public input or a witness.
    pub fn new(
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
    ) -> Self {
        Self {
            worker_secret,
            worker_public,
            job_id,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute nullifier from job_id and worker_secret
    /// `SubmitDeliverableV2` derives `spent_nullifier = poseidon_hash(1, 5, job_id, worker_secret)`
    /// and constrains it at instance 1. Domain 1 = `NULLIFIER`, tag 5 = the circuit's
    /// `SUBMIT_TAG`.
    ///
    /// This was `poseidon_hash([job_id, worker_secret])` — **no domain and no tag**, so the value
    /// could not equal the circuit's instance and the proof was unsatisfiable; it also violated
    /// `RC3` (every `poseidon_hash` prepends a domain constant). Two defects in one line, and the
    /// instance mismatch is the one that killed the suite (register OBL-C78).
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([
            pallas::Base::from(1u64),
            pallas::Base::from(5u64),
            self.job_id,
            self.worker_secret,
        ])
    }

    /// `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)`, domain 3, instance 5.
    pub fn compute_tx_binding(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce])
    }

    pub fn compute_public_inputs(&self) -> SubmitDeliverableV1PublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.worker_public.xy().expect("pk not identity");
        SubmitDeliverableV1PublicInputs {
            spent_nullifier: self.compute_nullifier(),
            job_id: self.job_id,
            worker_pub_x: ix,
            worker_pub_y: iy,
            tx_binding: self.compute_tx_binding(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.worker_public.xy().expect("pk not identity");
        // Must match `submit_deliverable.zk`'s `witness` block exactly:
        //   job_id, worker_secret, worker_pub_x, worker_pub_y, tx_commitment, tx_nonce, tx_binding
        // `spent_nullifier` is derived in-circuit from job_id + worker_secret, so it is an
        // instance, not a witness. `claim_id`, `deadline_block` and `current_block` were supplied
        // here and are declared nowhere — the vector was ten entries against seven (OBL-C78).
        vec![
            Witness::Base(Value::known(self.job_id)),
            Witness::Base(Value::known(self.worker_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.compute_tx_binding())), // tx_binding
        ]
    }
}

/// Create a SubmitDeliverable ZK proof
pub fn submit_deliverable_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SubmitDeliverableV1CallData,
) -> Result<(Proof, SubmitDeliverableV1PublicInputs)> {
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