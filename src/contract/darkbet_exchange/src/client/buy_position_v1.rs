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

//! DarkBet Exchange BuyPosition ZK proof generation

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

/// BuyPositionV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct BuyPositionV1PublicInputs {
    pub market_id: pallas::Base,
    pub owner_pub_x: pallas::Base,
    pub owner_pub_y: pallas::Base,
    pub outcome: pallas::Base,
    pub amount: pallas::Base,
    pub block_height: pallas::Base,
    pub derived_position_id: pallas::Base,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
}

impl BuyPositionV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.market_id,
            self.owner_pub_x,
            self.owner_pub_y,
            self.outcome,
            self.amount,
            self.block_height,
            self.derived_position_id,
            self.value_commit_x,
            self.value_commit_y,
        ]
    }
}

/// Input data for BuyPosition proof generation
#[derive(Debug, Clone)]
pub struct BuyPositionV1CallData {
    pub market_id: pallas::Base,
    pub owner_pub_x: pallas::Base,
    pub owner_pub_y: pallas::Base,
    pub outcome: u8,
    pub amount: u64,
    pub block_height: u64,
    pub value_blind: pallas::Scalar,
}

impl BuyPositionV1CallData {
    pub fn new(
        market_id: pallas::Base,
        owner_public: PublicKey,
        outcome: u8,
        amount: u64,
        block_height: u64,
        value_blind: pallas::Scalar,
    ) -> Self {
        let (ox, oy) = owner_public.xy();
        Self {
            market_id,
            owner_pub_x: ox,
            owner_pub_y: oy,
            outcome,
            amount,
            block_height,
            value_blind,
        }
    }

    pub fn compute_public_inputs(&self) -> BuyPositionV1PublicInputs {
        let derived_position_id = poseidon_hash([
            self.market_id,
            self.owner_pub_x,
            self.owner_pub_y,
            pallas::Base::from(self.outcome as u64),
            pallas::Base::from(self.amount),
            pallas::Base::from(self.block_height),
        ]);
        BuyPositionV1PublicInputs {
            market_id: self.market_id,
            owner_pub_x: self.owner_pub_x,
            owner_pub_y: self.owner_pub_y,
            outcome: pallas::Base::from(self.outcome as u64),
            amount: pallas::Base::from(self.amount),
            block_height: pallas::Base::from(self.block_height),
            derived_position_id,
            value_commit_x: pallas::Base::zero(),
            value_commit_y: pallas::Base::zero(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.market_id)),
            Witness::Base(Value::known(self.owner_pub_x)),
            Witness::Base(Value::known(self.owner_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.outcome as u64))),
            Witness::Base(Value::known(pallas::Base::from(self.amount))),
            Witness::Base(Value::known(pallas::Base::from(self.block_height))),
            // Private inputs
            Witness::Scalar(Value::known(self.value_blind)),
        ]
    }
}

/// Create a BuyPosition ZK proof
pub fn buy_position_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &BuyPositionV1CallData,
) -> Result<(Proof, BuyPositionV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}