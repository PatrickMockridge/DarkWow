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

//! Insurance Market PurchaseCoverageWithCapability ZK proof generation

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

/// PurchaseCoverageWithCapabilityV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct PurchaseCoverageWithCapabilityV1PublicInputs {
    pub buyer_pub_x: pallas::Base,
    pub buyer_pub_y: pallas::Base,
    pub required_capability_id: pallas::Base,
    pub capability_predicate_result: pallas::Base,
    pub derived_pub_x: pallas::Base,
    pub derived_pub_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PurchaseCoverageWithCapabilityV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.buyer_pub_x,
            self.buyer_pub_y,
            self.required_capability_id,
            self.capability_predicate_result,
            self.derived_pub_x,
            self.derived_pub_y,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for PurchaseCoverageWithCapability proof generation
#[derive(Debug, Clone)]
pub struct PurchaseCoverageWithCapabilityV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub buyer_secret: pallas::Base,
    pub buyer_pub_x: pallas::Base,
    pub buyer_pub_y: pallas::Base,
    pub required_capability_id: pallas::Base,
    pub capability_predicate_result: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PurchaseCoverageWithCapabilityV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        buyer_secret: pallas::Base,
        buyer_public: PublicKey,
        required_capability_id: pallas::Base,
        capability_predicate_result: pallas::Base,
    ) -> Self {
        let (bx, by) = buyer_public.xy().expect("pk not identity");
        Self {
            nullifier_k,
            buyer_secret,
            buyer_pub_x: bx,
            buyer_pub_y: by,
            required_capability_id,
            capability_predicate_result,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> PurchaseCoverageWithCapabilityV1PublicInputs {
        let derived_pub_x = poseidon_hash([
            self.buyer_pub_x,
            self.buyer_pub_y,
            self.required_capability_id,
            self.capability_predicate_result,
        ]);
        let derived_pub_y = poseidon_hash([
            self.buyer_secret,
            self.required_capability_id,
            self.capability_predicate_result,
        ]);
        PurchaseCoverageWithCapabilityV1PublicInputs {
            buyer_pub_x: self.buyer_pub_x,
            buyer_pub_y: self.buyer_pub_y,
            required_capability_id: self.required_capability_id,
            capability_predicate_result: self.capability_predicate_result,
            derived_pub_x,
            derived_pub_y,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Private inputs
            Witness::Scalar(Value::known(self.nullifier_k)),
            Witness::Base(Value::known(self.buyer_secret)),
            Witness::Base(Value::known(self.buyer_pub_x)),
            Witness::Base(Value::known(self.buyer_pub_y)),
            Witness::Base(Value::known(self.required_capability_id)),
            Witness::Base(Value::known(self.capability_predicate_result)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a PurchaseCoverageWithCapability ZK proof
pub fn purchase_coverage_with_capability_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PurchaseCoverageWithCapabilityV1CallData,
) -> Result<(Proof, PurchaseCoverageWithCapabilityV1PublicInputs)> {
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