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

//! Identity verify_capability_v1 ZK proof generation

use dwow::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// VerifyCapabilityV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct VerifyCapabilityPublicInputs {
    pub capability_id: pallas::Base,
    pub nullifier: pallas::Base,
    pub issuer_pub_x: pallas::Base,
    pub issuer_pub_y: pallas::Base,
    pub schema_hash: pallas::Base,
    pub predicate_result: pallas::Base,
}

impl VerifyCapabilityPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.capability_id,
            self.nullifier,
            self.issuer_pub_x,
            self.issuer_pub_y,
            self.schema_hash,
            self.predicate_result,
        ]
    }
}

/// Input data for verify_capability proof generation
#[derive(Debug, Clone)]
pub struct VerifyCapabilityCallData {
    pub credential_secret: pallas::Base,
    pub commitment: pallas::Base,
    pub attribute_value: pallas::Base,
    pub threshold: pallas::Base,
    pub capability_secret: pallas::Base,
    // Public inputs
    pub issuer_public: PublicKey,
    pub schema_hash: pallas::Base,
    pub capability_id: pallas::Base,
    pub predicate_result: bool,
}

impl VerifyCapabilityCallData {
    pub fn new(
        credential_secret: pallas::Base,
        commitment: pallas::Base,
        attribute_value: pallas::Base,
        threshold: pallas::Base,
        capability_secret: pallas::Base,
        issuer_public: PublicKey,
        schema_hash: pallas::Base,
        capability_id: pallas::Base,
        predicate_result: bool,
    ) -> Self {
        Self {
            credential_secret,
            commitment,
            attribute_value,
            threshold,
            capability_secret,
            issuer_public,
            schema_hash,
            capability_id,
            predicate_result,
        }
    }

    /// Compute nullifier from credential_secret and commitment
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.credential_secret, self.commitment])
    }

    /// Compute capability hash
    pub fn compute_capability(&self) -> pallas::Base {
        poseidon_hash([self.capability_secret, self.capability_id])
    }

    pub fn compute_public_inputs(&self) -> VerifyCapabilityPublicInputs {
        let (ix, iy) = self.issuer_public.xy();
        VerifyCapabilityPublicInputs {
            capability_id: self.capability_id,
            nullifier: self.compute_nullifier(),
            issuer_pub_x: ix,
            issuer_pub_y: iy,
            schema_hash: self.schema_hash,
            predicate_result: if self.predicate_result {
                pallas::Base::one()
            } else {
                pallas::Base::zero()
            },
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.issuer_public.xy();
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.capability_id)),
            Witness::Base(Value::known(self.compute_nullifier())),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.schema_hash)),
            Witness::Base(Value::known(if self.predicate_result { pallas::Base::one() } else { pallas::Base::zero() })),
            // Private inputs
            Witness::Base(Value::known(self.credential_secret)),
            Witness::Base(Value::known(self.commitment)),
            Witness::Base(Value::known(self.attribute_value)),
            Witness::Base(Value::known(self.threshold)),
            Witness::Base(Value::known(self.capability_secret)),
        ]
    }
}

/// Create a VerifyCapability ZK proof
pub fn create_verify_capability_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &VerifyCapabilityCallData,
) -> Result<(Proof, VerifyCapabilityPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}