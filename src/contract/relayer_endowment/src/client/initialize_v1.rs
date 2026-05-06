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
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// InitializeV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct InitializeV1PublicInputs {
    pub endowment_id: pallas::Base,
}

impl InitializeV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.endowment_id]
    }
}

/// Input data for initialize proof generation
#[derive(Debug, Clone)]
pub struct InitializeV1CallData {
    pub relayer_pub_x: pallas::Base,
    pub relayer_pub_y: pallas::Base,
    pub config_hash: pallas::Base,
    pub nonce: pallas::Base,
}

impl InitializeV1CallData {
    pub fn new(relayer_public: PublicKey, default_backer_cut_bp: u32, nonce: u64) -> Self {
        let (px, py) = relayer_public.xy();
        let config_hash = poseidon_hash([pallas::Base::from(default_backer_cut_bp as u64)]);
        Self { relayer_pub_x: px, relayer_pub_y: py, config_hash, nonce: pallas::Base::from(nonce) }
    }

    pub fn compute_public_inputs(&self) -> InitializeV1PublicInputs {
        let endowment_id = poseidon_hash([
            self.relayer_pub_x,
            self.relayer_pub_y,
            self.config_hash,
            self.nonce,
        ]);
        InitializeV1PublicInputs { endowment_id }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.relayer_pub_x)),
            Witness::Base(Value::known(self.relayer_pub_y)),
            Witness::Base(Value::known(self.config_hash)),
            Witness::Base(Value::known(self.nonce)),
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
