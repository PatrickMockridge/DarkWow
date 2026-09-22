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
use rand::SeedableRng;

/// DisputeV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct DisputeV1PublicInputs {
    pub spent_nullifier: pallas::Base,
    pub job_id: pallas::Base,
    pub disputer_pub_x: pallas::Base,
    pub disputer_pub_y: pallas::Base,
    pub dispute_reason_hash: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DisputeV1PublicInputs {
    /// Order must match `dispute.zk`'s `constrain_instance` sequence exactly:
    ///   spent_nullifier, job_id, disputer_pub_x, disputer_pub_y, dispute_reason_hash,
    ///   tx_binding, tx_nonce
    /// The vector carried `dao_escrow_bulla` at position 4, where the circuit constrains
    /// `dispute_reason_hash` — a different value in the position the verifier reads. The count
    /// happened to match, which is exactly what a count check cannot detect (register OBL-C78).
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.spent_nullifier,
            self.job_id,
            self.disputer_pub_x,
            self.disputer_pub_y,
            self.dispute_reason_hash,
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
    /// `DisputeV2` derives `spent_nullifier = poseidon_hash(1, 9, job_id, disputer_secret,
    /// dispute_reason_hash)` and constrains it at instance 1. Domain 1 = `NULLIFIER`, tag 9 = the
    /// circuit's `DISPUTE_TAG`. This was the three-argument hash with **no domain and no tag**, so
    /// it could not equal the circuit's instance and violated `RC3`.
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([
            pallas::Base::from(1u64),
            pallas::Base::from(9u64),
            self.job_id,
            self.disputer_secret,
            self.dispute_reason_hash,
        ])
    }

    /// `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)`, domain 3, instance 6.
    pub fn compute_tx_binding(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce])
    }

    pub fn compute_public_inputs(&self) -> DisputeV1PublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.disputer_public.xy().expect("pk not identity");
        DisputeV1PublicInputs {
            spent_nullifier: self.compute_nullifier(),
            job_id: self.job_id,
            disputer_pub_x: ix,
            disputer_pub_y: iy,
            dispute_reason_hash: self.dispute_reason_hash,
            tx_binding: self.compute_tx_binding(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.disputer_public.xy().expect("pk not identity");
        vec![
            // Must match `dispute.zk`'s `witness` block exactly:
            //   job_id, disputer_secret, disputer_pub_x, disputer_pub_y, dispute_reason_hash,
            //   tx_commitment, tx_nonce, tx_binding
            // `dao_escrow_bulla` was supplied here and is declared nowhere; `dispute_reason_hash`
            // sat before the public keys where the circuit places it after them (register
            // OBL-C78).
            Witness::Base(Value::known(self.job_id)),
            Witness::Base(Value::known(self.disputer_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.dispute_reason_hash)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.compute_tx_binding())), // tx_binding
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