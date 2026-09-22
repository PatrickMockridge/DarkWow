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

//! Labor Market confirm_delivery_v1 ZK proof generation

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

/// ConfirmDeliveryV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct ConfirmDeliveryV1PublicInputs {
    pub spent_nullifier: pallas::Base,
    pub job_id: pallas::Base,
    pub employer_pub_x: pallas::Base,
    pub employer_pub_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ConfirmDeliveryV1PublicInputs {
    /// Order must match `confirm_delivery.zk`'s `constrain_instance` sequence exactly:
    ///   spent_nullifier, job_id, employer_pub_x, employer_pub_y, tx_binding, tx_nonce
    /// The count already matched, but `spent_nullifier` sat at position 4 where the circuit leads
    /// with it — a right-length vector in the wrong order, which a count check cannot see and
    /// which no proof can survive (register OBL-C78).
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.spent_nullifier,
            self.job_id,
            self.employer_pub_x,
            self.employer_pub_y,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for confirm_delivery proof generation
#[derive(Debug, Clone)]
pub struct ConfirmDeliveryV1CallData {
    pub employer_secret: pallas::Base,
    // Public inputs
    pub employer_public: PublicKey,
    pub job_id: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ConfirmDeliveryV1CallData {
    pub fn new(employer_secret: pallas::Base, employer_public: PublicKey, job_id: pallas::Base) -> Self {
        Self {
            employer_secret,
            employer_public,
            job_id,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// `ConfirmDeliveryV2` derives `spent_nullifier = poseidon_hash(1, 4, job_id,
    /// employer_secret)` and constrains it at instance 1. Domain 1 = `NULLIFIER`, tag 4 = the
    /// circuit's `CONFIRM_TAG`. This was `poseidon_hash([job_id, employer_secret])` — no domain,
    /// no tag — so it could not equal the circuit's instance and violated `RC3`.
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([
            pallas::Base::from(1u64),
            pallas::Base::from(4u64),
            self.job_id,
            self.employer_secret,
        ])
    }

    /// `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)`, domain 3, instance 5.
    pub fn compute_tx_binding(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce])
    }

    pub fn compute_public_inputs(&self) -> ConfirmDeliveryV1PublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.employer_public.xy().expect("pk not identity");
        ConfirmDeliveryV1PublicInputs {
            spent_nullifier: self.compute_nullifier(),
            job_id: self.job_id,
            employer_pub_x: ix,
            employer_pub_y: iy,
            tx_binding: self.compute_tx_binding(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.employer_public.xy().expect("pk not identity");
        vec![
            // Must match circuit witness order:
            // job_id, employer_secret, employer_pub_x, employer_pub_y
            // (spent_nullifier is computed by the circuit, not provided as witness)
            Witness::Base(Value::known(self.job_id)),
            Witness::Base(Value::known(self.employer_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.compute_tx_binding())), // tx_binding
        ]
    }
}

/// Create a ConfirmDelivery ZK proof
pub fn confirm_delivery_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ConfirmDeliveryV1CallData,
) -> Result<(Proof, ConfirmDeliveryV1PublicInputs)> {
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