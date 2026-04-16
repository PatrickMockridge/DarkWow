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

//! Block Height Prediction create_position_v1 ZK proof generation

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

/// CreatePositionV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreatePositionV1PublicInputs {
    pub position_id: pallas::Base,
    pub market_id: pallas::Base,
}

impl CreatePositionV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.position_id, self.market_id]
    }
}

/// Input data for create_position proof generation
#[derive(Debug, Clone)]
pub struct CreatePositionV1CallData {
    pub player_pub_x: pallas::Base,
    pub player_pub_y: pallas::Base,
    pub market_id: pallas::Base,
    pub position_type: pallas::Base,
    pub stake_value: pallas::Base,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
}

impl CreatePositionV1CallData {
    pub fn new(
        player_pub: PublicKey,
        market_id: pallas::Base,
        position_type: u8,
        stake_value: u64,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
    ) -> Self {
        let (px, py) = player_pub.xy();
        Self {
            player_pub_x: px,
            player_pub_y: py,
            market_id,
            position_type: pallas::Base::from(position_type as u64),
            stake_value: pallas::Base::from(stake_value),
            secret_nonce,
            blind,
        }
    }

    pub fn compute_public_inputs(&self) -> CreatePositionV1PublicInputs {
        CreatePositionV1PublicInputs { position_id: pallas::Base::zero(), market_id: self.market_id }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.player_pub_x)),
            Witness::Base(Value::known(self.player_pub_y)),
            Witness::Base(Value::known(self.market_id)),
            Witness::Base(Value::known(self.position_type)),
            Witness::Base(Value::known(self.stake_value)),
            // Private inputs
            Witness::Base(Value::known(self.secret_nonce)),
            Witness::Base(Value::known(self.blind)),
        ]
    }
}

/// Create a CreatePosition ZK proof
pub fn create_position_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreatePositionV1CallData,
) -> Result<(Proof, CreatePositionV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}