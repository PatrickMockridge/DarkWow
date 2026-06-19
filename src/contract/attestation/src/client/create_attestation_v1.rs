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

//! Attestation create_attestation_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use rand::rngs::OsRng;

/// CreateAttestationV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreateAttestationV1PublicInputs {
    pub attestor_pub_x: pallas::Base,
    pub attestor_pub_y: pallas::Base,
    pub tx_commitment: pallas::Base,
}

impl CreateAttestationV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.attestor_pub_x, self.attestor_pub_y, self.tx_commitment]
    }
}

/// Input data for create_attestation proof generation
#[derive(Debug, Clone)]
pub struct CreateAttestationV1CallData {
    pub attestor_secret: pallas::Base,
    // Public inputs
    pub attestor_public: PublicKey,
    pub tx_commitment: pallas::Base,
}

impl CreateAttestationV1CallData {
    pub fn new(attestor_secret: pallas::Base, attestor_public: PublicKey) -> Self {
        Self { attestor_secret, attestor_public, tx_commitment: pallas::Base::zero() }
    }

    pub fn compute_public_inputs(&self) -> CreateAttestationV1PublicInputs {
        let (ix, iy) = self.attestor_public.xy();
        CreateAttestationV1PublicInputs { attestor_pub_x: ix, attestor_pub_y: iy, tx_commitment: self.tx_commitment }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.attestor_public.xy();
        vec![
            // Must match circuit witness order:
            // attestor_secret, attestor_pub_x, attestor_pub_y
            Witness::Base(Value::known(self.attestor_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
        ]
    }
}

/// Create a CreateAttestation ZK proof
pub fn create_attestation_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateAttestationV1CallData,
) -> Result<(Proof, CreateAttestationV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}