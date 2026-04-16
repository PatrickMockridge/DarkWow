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

//! Relayer Endowment initialize_v1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use rand::rngs::OsRng;

/// InitializeV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct InitializeV1PublicInputs {
    pub relayer_pub_x: pallas::Base,
    pub relayer_pub_y: pallas::Base,
    pub endowment_id: pallas::Base,
}

impl InitializeV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.relayer_pub_x, self.relayer_pub_y, self.endowment_id]
    }
}

/// Input data for initialize proof generation
#[derive(Debug, Clone)]
pub struct InitializeV1CallData {
    pub relayer_pub_x: pallas::Base,
    pub relayer_pub_y: pallas::Base,
    pub endowment_id: pallas::Base,
    pub secret: pallas::Base,
}

impl InitializeV1CallData {
    pub fn new(relayer_public: PublicKey, endowment_id: pallas::Base, secret: pallas::Base) -> Self {
        let (px, py) = relayer_public.xy();
        Self { relayer_pub_x: px, relayer_pub_y: py, endowment_id, secret }
    }

    pub fn compute_public_inputs(&self) -> InitializeV1PublicInputs {
        InitializeV1PublicInputs {
            relayer_pub_x: self.relayer_pub_x,
            relayer_pub_y: self.relayer_pub_y,
            endowment_id: self.endowment_id,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.relayer_pub_x)),
            Witness::Base(Value::known(self.relayer_pub_y)),
            Witness::Base(Value::known(self.endowment_id)),
            // Private inputs
            Witness::Base(Value::known(self.secret)),
        ]
    }
}

/// Create a Initialize ZK proof
pub fn initialize_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &InitializeV1CallData,
) -> Result<(Proof, InitializeV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}