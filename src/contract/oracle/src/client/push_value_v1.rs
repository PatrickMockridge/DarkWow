/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Oracle push_value_v1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// PushValueV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct PushValueV1PublicInputs {
    pub oracle_id: pallas::Base,
    pub value: pallas::Base,
}

impl PushValueV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.oracle_id, self.value]
    }
}

/// Input data for push_value proof generation
#[derive(Debug, Clone)]
pub struct PushValueV1CallData {
    pub oracle_id: pallas::Base,
    pub oracle_secret: pallas::Base,
    // Public inputs
    pub oracle_public: PublicKey,
    pub value: pallas::Base,
}

impl PushValueV1CallData {
    pub fn new(
        oracle_id: pallas::Base,
        oracle_secret: pallas::Base,
        oracle_public: PublicKey,
        value: pallas::Base,
    ) -> Self {
        Self { oracle_id, oracle_secret, oracle_public, value }
    }

    pub fn compute_public_inputs(&self) -> PushValueV1PublicInputs {
        PushValueV1PublicInputs { oracle_id: self.oracle_id, value: self.value }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.oracle_public.xy();
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.oracle_id)),
            Witness::Base(Value::known(self.value)),
            // Private inputs
            Witness::Base(Value::known(self.oracle_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
        ]
    }
}

/// Create a PushValue ZK proof
pub fn push_value_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PushValueV1CallData,
) -> Result<(Proof, PushValueV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}