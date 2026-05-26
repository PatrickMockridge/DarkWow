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

//! Oracle attest_value_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// AttestValueV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct AttestValueV1PublicInputs {
    pub oracle_id: pallas::Base,
    pub attestation_id: pallas::Base,
    pub predicate: pallas::Base,
    pub threshold: pallas::Base,
}

impl AttestValueV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.oracle_id,
            self.attestation_id,
            self.predicate,
            self.threshold,
        ]
    }
}

/// Input data for attest_value proof generation
#[derive(Debug, Clone)]
pub struct AttestValueV1CallData {
    pub oracle_id: pallas::Base,
    pub attestation_id: pallas::Base,
    pub oracle_secret: pallas::Base,
    pub predicate: pallas::Base,
    pub threshold: pallas::Base,
    pub value: pallas::Base,
    // Public inputs
    pub oracle_public: PublicKey,
}

impl AttestValueV1CallData {
    pub fn new(
        oracle_id: pallas::Base,
        attestation_id: pallas::Base,
        oracle_secret: pallas::Base,
        predicate: pallas::Base,
        threshold: pallas::Base,
        value: pallas::Base,
        oracle_public: PublicKey,
    ) -> Self {
        Self {
            oracle_id,
            attestation_id,
            oracle_secret,
            predicate,
            threshold,
            value,
            oracle_public,
        }
    }

    pub fn compute_public_inputs(&self) -> AttestValueV1PublicInputs {
        AttestValueV1PublicInputs {
            oracle_id: self.oracle_id,
            attestation_id: self.attestation_id,
            predicate: self.predicate,
            threshold: self.threshold,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.oracle_public.xy();
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.oracle_id)),
            Witness::Base(Value::known(self.attestation_id)),
            Witness::Base(Value::known(self.predicate)),
            Witness::Base(Value::known(self.threshold)),
            // Private inputs
            Witness::Base(Value::known(self.oracle_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.value)),
        ]
    }
}

/// Create an AttestValue ZK proof
pub fn attest_value_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &AttestValueV1CallData,
) -> Result<(Proof, AttestValueV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}