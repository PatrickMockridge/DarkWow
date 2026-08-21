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

//! Labor Market submit_git_deliverable_v1 ZK proof generation

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

/// SubmitGitDeliverableV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SubmitGitDeliverableV1PublicInputs {
    pub job_id: pallas::Base,
    pub claim_id: pallas::Base,
    pub worker_pub_x: pallas::Base,
    pub worker_pub_y: pallas::Base,
    pub spent_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SubmitGitDeliverableV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.job_id,
            self.claim_id,
            self.worker_pub_x,
            self.worker_pub_y,
            self.spent_nullifier,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for submit_git_deliverable proof generation
#[derive(Debug, Clone)]
pub struct SubmitGitDeliverableV1CallData {
    pub worker_secret: pallas::Base,
    // Public inputs
    pub worker_public: PublicKey,
    pub job_id: pallas::Base,
    pub claim_id: pallas::Base,
    pub deadline_block: pallas::Base,
    pub current_block: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SubmitGitDeliverableV1CallData {
    pub fn new(
        worker_secret: pallas::Base,
        worker_public: PublicKey,
        job_id: pallas::Base,
        claim_id: pallas::Base,
        deadline_block: pallas::Base,
        current_block: pallas::Base,
    ) -> Self {
        Self {
            worker_secret,
            worker_public,
            job_id,
            claim_id,
            deadline_block,
            current_block,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute nullifier from job_id and worker_secret
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.job_id, self.worker_secret])
    }

    pub fn compute_public_inputs(&self) -> SubmitGitDeliverableV1PublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.worker_public.xy().expect("pk not identity");
        SubmitGitDeliverableV1PublicInputs {
            job_id: self.job_id,
            claim_id: self.claim_id,
            worker_pub_x: ix,
            worker_pub_y: iy,
            spent_nullifier: self.compute_nullifier(),
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.worker_public.xy().expect("pk not identity");
        vec![
            // Must match circuit witness order:
            // job_id, claim_id, worker_secret, worker_pub_x, worker_pub_y,
            // deadline_block, current_block
            // (spent_nullifier is computed by the circuit, not provided as witness)
            Witness::Base(Value::known(self.job_id)),
            Witness::Base(Value::known(self.claim_id)),
            Witness::Base(Value::known(self.worker_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.deadline_block)),
            Witness::Base(Value::known(self.current_block)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a SubmitGitDeliverable ZK proof
pub fn submit_git_deliverable_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SubmitGitDeliverableV1CallData,
) -> Result<(Proof, SubmitGitDeliverableV1PublicInputs)> {
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