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

//! Tender create_tender_v1 ZK proof generation

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

/// CreateTenderV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreateTenderV1PublicInputs {
    pub requester_pub_x: pallas::Base,
    pub requester_pub_y: pallas::Base,
}

impl CreateTenderV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.requester_pub_x, self.requester_pub_y]
    }
}

/// Input data for create_tender proof generation
#[derive(Debug, Clone)]
pub struct CreateTenderV1CallData {
    pub requester_secret: pallas::Base,
    // Public inputs
    pub requester_public: PublicKey,
}

impl CreateTenderV1CallData {
    pub fn new(requester_secret: pallas::Base, requester_public: PublicKey) -> Self {
        Self { requester_secret, requester_public }
    }

    pub fn compute_public_inputs(&self) -> CreateTenderV1PublicInputs {
        let (ix, iy) = self.requester_public.xy();
        CreateTenderV1PublicInputs { requester_pub_x: ix, requester_pub_y: iy }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.requester_public.xy();
        vec![
            // Must match circuit witness order:
            // requester_secret, requester_pub_x, requester_pub_y
            Witness::Base(Value::known(self.requester_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
        ]
    }
}

/// Create a CreateTender ZK proof
pub fn create_tender_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateTenderV1CallData,
) -> Result<(Proof, CreateTenderV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}