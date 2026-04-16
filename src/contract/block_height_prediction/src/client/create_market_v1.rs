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

//! Block Height Prediction create_market_v1 ZK proof generation

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

/// CreateMarketV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreateMarketV1PublicInputs {
    pub market_id: pallas::Base,
    pub collateral_commit_x: pallas::Base,
    pub collateral_commit_y: pallas::Base,
}

impl CreateMarketV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.market_id, self.collateral_commit_x, self.collateral_commit_y]
    }
}

/// Input data for create_market proof generation
#[derive(Debug, Clone)]
pub struct CreateMarketV1CallData {
    pub creator_pub_x: pallas::Base,
    pub creator_pub_y: pallas::Base,
    pub start_block: pallas::Base,
    pub end_block: pallas::Base,
    pub resolve_block: pallas::Base,
    pub collateral_token: pallas::Base,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
}

impl CreateMarketV1CallData {
    pub fn new(
        creator_pub: PublicKey,
        start_block: u64,
        end_block: u64,
        resolve_block: u64,
        collateral_token: pallas::Base,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
    ) -> Self {
        let (px, py) = creator_pub.xy();
        Self {
            creator_pub_x: px,
            creator_pub_y: py,
            start_block: pallas::Base::from(start_block),
            end_block: pallas::Base::from(end_block),
            resolve_block: pallas::Base::from(resolve_block),
            collateral_token,
            secret_nonce,
            blind,
        }
    }

    pub fn compute_public_inputs(&self) -> CreateMarketV1PublicInputs {
        let market_id = poseidon_hash([
            self.creator_pub_x,
            self.creator_pub_y,
            self.start_block,
            self.end_block,
            self.resolve_block,
            self.collateral_token,
            self.secret_nonce,
            self.blind,
        ]);
        CreateMarketV1PublicInputs {
            market_id,
            collateral_commit_x: pallas::Base::zero(),
            collateral_commit_y: pallas::Base::zero(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.creator_pub_x)),
            Witness::Base(Value::known(self.creator_pub_y)),
            Witness::Base(Value::known(self.start_block)),
            Witness::Base(Value::known(self.end_block)),
            Witness::Base(Value::known(self.resolve_block)),
            Witness::Base(Value::known(self.collateral_token)),
            // Private inputs
            Witness::Base(Value::known(self.secret_nonce)),
            Witness::Base(Value::known(self.blind)),
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