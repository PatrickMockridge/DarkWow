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

//! DarkBet Exchange CreateMarket ZK proof generation

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

/// CreateMarketV1 circuit public inputs (only 1 - matching what circuit exposes)
#[derive(Debug, Clone)]
pub struct CreateMarketV1PublicInputs {
    pub derived_market_id: pallas::Base,
}

impl CreateMarketV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.derived_market_id]
    }
}

/// Input data for CreateMarket proof generation
#[derive(Debug, Clone)]
pub struct CreateMarketV1CallData {
    pub creator_pub_x: pallas::Base,
    pub creator_pub_y: pallas::Base,
    pub close_block: u64,
    pub block_height: u64,
    pub nonce: u64,
}

impl CreateMarketV1CallData {
    pub fn new(
        creator_public: PublicKey,
        close_block: u64,
        block_height: u64,
        nonce: u64,
    ) -> Self {
        let (cx, cy) = creator_public.xy();
        Self { creator_pub_x: cx, creator_pub_y: cy, close_block, block_height, nonce }
    }

    pub fn compute_public_inputs(&self) -> CreateMarketV1PublicInputs {
        // NOTE: nonce is NOT included in the hash - the circuit doesn't use it
        let derived_market_id = poseidon_hash([
            self.creator_pub_x,
            self.creator_pub_y,
            pallas::Base::from(self.close_block),
            pallas::Base::from(self.block_height),
        ]);
        CreateMarketV1PublicInputs { derived_market_id }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.creator_pub_x)),
            Witness::Base(Value::known(self.creator_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.close_block))),
            Witness::Base(Value::known(pallas::Base::from(self.block_height))),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
        ]
    }
}

/// Create a CreateMarket ZK proof
pub fn create_market_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateMarketV1CallData,
) -> Result<(Proof, CreateMarketV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}