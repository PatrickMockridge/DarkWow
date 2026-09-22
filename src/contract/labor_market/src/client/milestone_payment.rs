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

/// MilestonePaymentV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct MilestonePaymentV1PublicInputs {
    pub spent_nullifier: pallas::Base,
    pub job_id: pallas::Base,
    pub employer_pub_x: pallas::Base,
    pub employer_pub_y: pallas::Base,
    pub milestone_payment_amount: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl MilestonePaymentV1PublicInputs {
    /// Order must match `milestone_payment.zk`'s `constrain_instance` sequence exactly:
    ///   spent_nullifier, job_id, employer_pub_x, employer_pub_y, milestone_payment_amount,
    ///   tx_binding, tx_nonce
    /// `spent_nullifier` sat at position 5 where the circuit leads with it (register OBL-C78).
    /// This is the circuit `ConfirmMilestoneV1` must be dispatched to; the host published
    /// `ConfirmDeliveryV2` for it instead, so the proof below was for a namespace the verifier
    /// never looked at (see `get_metadata`).
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.spent_nullifier,
            self.job_id,
            self.employer_pub_x,
            self.employer_pub_y,
            self.milestone_payment_amount,
            self.tx_binding,
            self.tx_nonce,
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
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl MilestonePaymentV1CallData {
    /// `last_milestone_block`, `current_block` and `deadline_block` used to be taken here and
    /// threaded into `to_witnesses`, where `milestone_payment.zk` declares none of them — three of
    /// the eight extra entries (register OBL-C78). None of the three is an instance either, and
    /// the params carry none of them, so this call proves nothing about the deadline despite the
    /// exec comment claiming the deadline is what authorises the release.
    pub fn new(
        job_id: pallas::Base,
        milestone_payment_amount: pallas::Base,
        employer_secret: pallas::Base,
        employer_public: PublicKey,
    ) -> Self {
        Self {
            job_id,
            milestone_payment_amount,
            employer_secret,
            employer_public,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute nullifier from job_id and employer_secret
    /// `MilestonePaymentV2` derives `spent_nullifier = poseidon_hash(1, 2, job_id,
    /// employer_secret)` and constrains it at instance 1. Domain 1 = `NULLIFIER`, tag 2 = the
    /// circuit's `MILESTONE_TAG`. This was `poseidon_hash([job_id, employer_secret])` — no domain,
    /// no tag — so it could not equal the circuit's instance and violated `RC3`.
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([
            pallas::Base::from(1u64),
            pallas::Base::from(2u64),
            self.job_id,
            self.employer_secret,
        ])
    }

    /// `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)`, domain 3, instance 6.
    pub fn compute_tx_binding(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce])
    }

    pub fn compute_public_inputs(&self) -> MilestonePaymentV1PublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.employer_public.xy().expect("pk not identity");
        MilestonePaymentV1PublicInputs {
            spent_nullifier: self.compute_nullifier(),
            job_id: self.job_id,
            employer_pub_x: ix,
            employer_pub_y: iy,
            milestone_payment_amount: self.milestone_payment_amount,
            tx_binding: self.compute_tx_binding(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.employer_public.xy().expect("pk not identity");
        // Must match `milestone_payment.zk`'s `witness` block exactly:
        //   job_id, employer_secret, employer_pub_x, employer_pub_y, milestone_payment_amount,
        //   tx_commitment, tx_nonce, tx_binding
        // This vector listed its public inputs AGAIN as private witnesses and repeated
        // `employer_pub_x`/`employer_pub_y`/`milestone_payment_amount` twice over — sixteen
        // entries against eight (register OBL-C78). `spent_nullifier` is derived in-circuit, so it
        // is an instance, not a witness. `last_milestone_block`, `current_block` and
        // `deadline_block` are declared nowhere; the deadline is not a witness, so this circuit
        // does not prove it passed.
        vec![
            Witness::Base(Value::known(self.job_id)),
            Witness::Base(Value::known(self.employer_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.milestone_payment_amount)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.compute_tx_binding())), // tx_binding
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