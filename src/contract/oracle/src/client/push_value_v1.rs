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

//! Oracle push_value_v1 ZK proof generation

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

/// PushValueV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct PushValueV1PublicInputs {
    pub oracle_id: pallas::Base,
    pub value: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PushValueV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.oracle_id, self.value, self.tx_binding, self.tx_nonce]
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
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PushValueV1CallData {
    pub fn new(
        oracle_id: pallas::Base,
        oracle_secret: pallas::Base,
        oracle_public: PublicKey,
        value: pallas::Base,
    ) -> Self {
        Self { oracle_id, oracle_secret, oracle_public, value, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    pub fn compute_public_inputs(&self) -> PushValueV1PublicInputs {
        PushValueV1PublicInputs { oracle_id: self.oracle_id, value: self.value, tx_binding: pallas::Base::zero(), tx_nonce: self.tx_nonce }
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
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
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