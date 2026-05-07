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

//! Pool Stake CreatePool ZK proof generation

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

/// CreatePoolV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreatePoolV1PublicInputs {
    pub creator_pub_x: pallas::Base,
    pub creator_pub_y: pallas::Base,
    pub pool_config_hash: pallas::Base,
    pub nonce: pallas::Base,
    pub derived_pool_id: pallas::Base,
}

impl CreatePoolV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // Only constrain_instance values (derived_pool_id is the sole public instance)
        vec![self.derived_pool_id]
    }
}

/// Input data for CreatePool proof generation
#[derive(Debug, Clone)]
pub struct CreatePoolV1CallData {
    pub creator_pub_x: pallas::Base,
    pub creator_pub_y: pallas::Base,
    pub pool_config_hash: pallas::Base,
    pub nonce: u64,
}

impl CreatePoolV1CallData {
    pub fn new(
        creator_public: PublicKey,
        pool_config_hash: pallas::Base,
        nonce: u64,
    ) -> Self {
        let (cx, cy) = creator_public.xy();
        Self { creator_pub_x: cx, creator_pub_y: cy, pool_config_hash, nonce }
    }

    pub fn compute_public_inputs(&self) -> CreatePoolV1PublicInputs {
        let derived_pool_id = poseidon_hash([
            self.creator_pub_x,
            self.creator_pub_y,
            self.pool_config_hash,
            pallas::Base::from(self.nonce),
        ]);
        CreatePoolV1PublicInputs {
            creator_pub_x: self.creator_pub_x,
            creator_pub_y: self.creator_pub_y,
            pool_config_hash: self.pool_config_hash,
            nonce: pallas::Base::from(self.nonce),
            derived_pool_id,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.creator_pub_x)),
            Witness::Base(Value::known(self.creator_pub_y)),
            Witness::Base(Value::known(self.pool_config_hash)),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
        ]
    }
}

/// Create a CreatePool ZK proof
pub fn create_pool_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreatePoolV1CallData,
) -> Result<(Proof, CreatePoolV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}