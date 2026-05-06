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

//! Pool Stake JoinPool ZK proof generation

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

/// JoinPoolV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct JoinPoolV1PublicInputs {
    pub pool_id: pallas::Base,
    pub member_pub_x: pallas::Base,
    pub member_pub_y: pallas::Base,
    pub stake_amount: pallas::Base,
    pub token_id: pallas::Base,
    pub nonce: pallas::Base,
    pub derived_member_id: pallas::Base,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
}

impl JoinPoolV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // Only constrain_instance values:
        // derived_member_id, value_commit_x, value_commit_y
        vec![self.derived_member_id, self.value_commit_x, self.value_commit_y]
    }
}

/// Input data for JoinPool proof generation
#[derive(Debug, Clone)]
pub struct JoinPoolV1CallData {
    pub pool_id: pallas::Base,
    pub member_pub_x: pallas::Base,
    pub member_pub_y: pallas::Base,
    pub stake_amount: u64,
    pub token_id: pallas::Base,
    pub nonce: u64,
    pub value_blind: pallas::Scalar,
}

impl JoinPoolV1CallData {
    pub fn new(
        pool_id: pallas::Base,
        member_public: PublicKey,
        stake_amount: u64,
        token_id: pallas::Base,
        nonce: u64,
        value_blind: pallas::Scalar,
    ) -> Self {
        let (mx, my) = member_public.xy();
        Self {
            pool_id,
            member_pub_x: mx,
            member_pub_y: my,
            stake_amount,
            token_id,
            nonce,
            value_blind,
        }
    }

    pub fn compute_public_inputs(&self) -> JoinPoolV1PublicInputs {
        let derived_member_id = poseidon_hash([
            self.pool_id,
            self.member_pub_x,
            self.member_pub_y,
            pallas::Base::from(self.stake_amount),
            pallas::Base::from(self.nonce),
        ]);
        JoinPoolV1PublicInputs {
            pool_id: self.pool_id,
            member_pub_x: self.member_pub_x,
            member_pub_y: self.member_pub_y,
            stake_amount: pallas::Base::from(self.stake_amount),
            token_id: self.token_id,
            nonce: pallas::Base::from(self.nonce),
            derived_member_id,
            value_commit_x: pallas::Base::zero(),
            value_commit_y: pallas::Base::zero(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.pool_id)),
            Witness::Base(Value::known(self.member_pub_x)),
            Witness::Base(Value::known(self.member_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.stake_amount))),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            // Private inputs
            Witness::Scalar(Value::known(self.value_blind)),
        ]
    }
}

/// Create a JoinPool ZK proof
pub fn join_pool_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &JoinPoolV1CallData,
) -> Result<(Proof, JoinPoolV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}