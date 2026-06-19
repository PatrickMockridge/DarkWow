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

//! Insurance Market UnderwriteWithCapability ZK proof generation

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

/// UnderwriteWithCapabilityV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct UnderwriteWithCapabilityV1PublicInputs {
    pub underwriter_pub_x: pallas::Base,
    pub underwriter_pub_y: pallas::Base,
    pub required_capability_id: pallas::Base,
    pub capability_predicate_result: pallas::Base,
    pub derived_pub_x: pallas::Base,
    pub derived_pub_y: pallas::Base,
    pub tx_commitment: pallas::Base,
}

impl UnderwriteWithCapabilityV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.underwriter_pub_x,
            self.underwriter_pub_y,
            self.required_capability_id,
            self.capability_predicate_result,
            self.derived_pub_x,
            self.derived_pub_y,
            self.tx_commitment,
        ]
    }
}

/// Input data for UnderwriteWithCapability proof generation
#[derive(Debug, Clone)]
pub struct UnderwriteWithCapabilityV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub underwriter_secret: pallas::Base,
    pub underwriter_pub_x: pallas::Base,
    pub underwriter_pub_y: pallas::Base,
    pub required_capability_id: pallas::Base,
    pub capability_predicate_result: pallas::Base,
    pub tx_commitment: pallas::Base,
}

impl UnderwriteWithCapabilityV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        underwriter_secret: pallas::Base,
        underwriter_public: PublicKey,
        required_capability_id: pallas::Base,
        capability_predicate_result: pallas::Base,
    ) -> Self {
        let (ux, uy) = underwriter_public.xy();
        Self {
            nullifier_k,
            underwriter_secret,
            underwriter_pub_x: ux,
            underwriter_pub_y: uy,
            required_capability_id,
            capability_predicate_result,
            tx_commitment: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> UnderwriteWithCapabilityV1PublicInputs {
        let derived_pub_x = poseidon_hash([
            self.underwriter_pub_x,
            self.underwriter_pub_y,
            self.required_capability_id,
            self.capability_predicate_result,
        ]);
        let derived_pub_y = poseidon_hash([
            self.underwriter_secret,
            self.required_capability_id,
            self.capability_predicate_result,
        ]);
        UnderwriteWithCapabilityV1PublicInputs {
            underwriter_pub_x: self.underwriter_pub_x,
            underwriter_pub_y: self.underwriter_pub_y,
            required_capability_id: self.required_capability_id,
            capability_predicate_result: self.capability_predicate_result,
            derived_pub_x,
            derived_pub_y,
            tx_commitment: self.tx_commitment,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Private inputs
            Witness::Scalar(Value::known(self.nullifier_k)),
            Witness::Base(Value::known(self.underwriter_secret)),
            Witness::Base(Value::known(self.underwriter_pub_x)),
            Witness::Base(Value::known(self.underwriter_pub_y)),
            Witness::Base(Value::known(self.required_capability_id)),
            Witness::Base(Value::known(self.capability_predicate_result)),
        ]
    }
}

/// Create an UnderwriteWithCapability ZK proof
pub fn underwrite_with_capability_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &UnderwriteWithCapabilityV1CallData,
) -> Result<(Proof, UnderwriteWithCapabilityV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}