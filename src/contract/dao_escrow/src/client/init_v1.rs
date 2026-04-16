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

//! DAO-Escrow Init ZK proof generation

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

/// InitV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct InitV1PublicInputs {
    pub dao_bulla: pallas::Base,
    pub owner_pub_x: pallas::Base,
    pub owner_pub_y: pallas::Base,
    pub endowment_token_id: pallas::Base,
    pub endowment_bulla: pallas::Base,
}

impl InitV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.dao_bulla,
            self.owner_pub_x,
            self.owner_pub_y,
            self.endowment_token_id,
            self.endowment_bulla,
        ]
    }
}

/// Input data for Init proof generation
#[derive(Debug, Clone)]
pub struct InitV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub dao_bulla: pallas::Base,
    pub owner_secret: pallas::Base,
    pub owner_pub_x: pallas::Base,
    pub owner_pub_y: pallas::Base,
    pub endowment_token_id: pallas::Base,
    pub bulla_blind: pallas::Scalar,
}

impl InitV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        dao_bulla: pallas::Base,
        owner_public: PublicKey,
        endowment_token_id: pallas::Base,
        bulla_blind: pallas::Scalar,
    ) -> Self {
        let (ox, oy) = owner_public.xy();
        Self {
            nullifier_k,
            dao_bulla,
            owner_secret: pallas::Base::zero(), // Must be provided separately if needed
            owner_pub_x: ox,
            owner_pub_y: oy,
            endowment_token_id,
            bulla_blind,
        }
    }

    pub fn compute_public_inputs(&self) -> InitV1PublicInputs {
        let endowment_bulla = poseidon_hash([
            self.dao_bulla,
            self.owner_pub_x,
            self.owner_pub_y,
            self.endowment_token_id,
        ]);
        InitV1PublicInputs {
            dao_bulla: self.dao_bulla,
            owner_pub_x: self.owner_pub_x,
            owner_pub_y: self.owner_pub_y,
            endowment_token_id: self.endowment_token_id,
            endowment_bulla,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Private inputs
            Witness::Scalar(Value::known(self.nullifier_k)),
            Witness::Base(Value::known(self.dao_bulla)),
            Witness::Base(Value::known(self.owner_secret)),
            Witness::Base(Value::known(self.owner_pub_x)),
            Witness::Base(Value::known(self.owner_pub_y)),
            Witness::Base(Value::known(self.endowment_token_id)),
            Witness::Scalar(Value::known(self.bulla_blind)),
        ]
    }
}

/// Create an Init ZK proof
pub fn init_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &InitV1CallData,
) -> Result<(Proof, InitV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}