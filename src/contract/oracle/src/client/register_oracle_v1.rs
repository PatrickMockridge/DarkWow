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

//! Oracle register_oracle_v1 ZK proof generation

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

/// RegisterOracleV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct RegisterOracleV1PublicInputs {
    pub oracle_pub_x: pallas::Base,
    pub oracle_pub_y: pallas::Base,
}

impl RegisterOracleV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.oracle_pub_x, self.oracle_pub_y]
    }
}

/// Input data for register_oracle proof generation
#[derive(Debug, Clone)]
pub struct RegisterOracleV1CallData {
    pub oracle_secret: pallas::Base,
    // Public inputs
    pub oracle_public: PublicKey,
}

impl RegisterOracleV1CallData {
    pub fn new(oracle_secret: pallas::Base, oracle_public: PublicKey) -> Self {
        Self { oracle_secret, oracle_public }
    }

    pub fn compute_public_inputs(&self) -> RegisterOracleV1PublicInputs {
        let (ix, iy) = self.oracle_public.xy();
        RegisterOracleV1PublicInputs { oracle_pub_x: ix, oracle_pub_y: iy }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.oracle_public.xy();
        vec![
            // Witnesses (must match circuit order: oracle_secret, oracle_pub_x, oracle_pub_y)
            Witness::Base(Value::known(self.oracle_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
        ]
    }
}

/// Create a RegisterOracle ZK proof
pub fn register_oracle_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RegisterOracleV1CallData,
) -> Result<(Proof, RegisterOracleV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}