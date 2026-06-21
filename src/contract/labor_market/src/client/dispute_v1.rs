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

//! Labor Market dispute_v1 ZK proof generation

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

/// DisputeV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct DisputeV1PublicInputs {
    pub job_id: pallas::Base,
    pub disputer_pub_x: pallas::Base,
    pub disputer_pub_y: pallas::Base,
    pub dao_escrow_bulla: pallas::Base,
    pub spent_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DisputeV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.job_id,
            self.disputer_pub_x,
            self.disputer_pub_y,
            self.dao_escrow_bulla,
            self.spent_nullifier,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for dispute proof generation
#[derive(Debug, Clone)]
pub struct DisputeV1CallData {
    pub job_id: pallas::Base,
    pub disputer_secret: pallas::Base,
    pub dispute_reason_hash: pallas::Base,
    pub dao_escrow_bulla: pallas::Base,
    // Public inputs
    pub disputer_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DisputeV1CallData {
    pub fn new(
        job_id: pallas::Base,
        disputer_secret: pallas::Base,
        dispute_reason_hash: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        disputer_public: PublicKey,
    ) -> Self {
        Self {
            job_id,
            disputer_secret,
            dispute_reason_hash,
            dao_escrow_bulla,
            disputer_public,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute nullifier from job_id, disputer_secret, and dispute_reason_hash
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.job_id, self.disputer_secret, self.dispute_reason_hash])
    }

    pub fn compute_public_inputs(&self) -> DisputeV1PublicInputs {
        let (ix, iy) = self.disputer_public.xy();
        DisputeV1PublicInputs {
            job_id: self.job_id,
            disputer_pub_x: ix,
            disputer_pub_y: iy,
            dao_escrow_bulla: self.dao_escrow_bulla,
            spent_nullifier: self.compute_nullifier(),
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.disputer_public.xy();
        vec![
            // Must match circuit witness order:
            // job_id, disputer_secret, dispute_reason_hash, dao_escrow_bulla,
            // disputer_pub_x, disputer_pub_y
            // (spent_nullifier is computed by the circuit, not provided as witness)
            Witness::Base(Value::known(self.job_id)),
            Witness::Base(Value::known(self.disputer_secret)),
            Witness::Base(Value::known(self.dispute_reason_hash)),
            Witness::Base(Value::known(self.dao_escrow_bulla)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a Dispute ZK proof
pub fn dispute_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &DisputeV1CallData,
) -> Result<(Proof, DisputeV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}