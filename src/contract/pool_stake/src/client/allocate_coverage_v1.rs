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

//! Pool Stake AllocateCoverage ZK proof generation

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

/// AllocateCoverageV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct AllocateCoverageV1PublicInputs {
    pub pool_id: pallas::Base,
    pub member_pub_x: pallas::Base,
    pub member_pub_y: pallas::Base,
    pub coverage_amount: pallas::Base,
    pub withdrawal_id: pallas::Base,
    pub nonce: pallas::Base,
    pub derived_allocation_id: pallas::Base,
}

impl AllocateCoverageV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // Only constrain_instance values (derived_allocation_id is the sole public instance)
        vec![self.derived_allocation_id]
    }
}

/// Input data for AllocateCoverage proof generation
#[derive(Debug, Clone)]
pub struct AllocateCoverageV1CallData {
    pub pool_id: pallas::Base,
    pub member_pub_x: pallas::Base,
    pub member_pub_y: pallas::Base,
    pub coverage_amount: u64,
    pub withdrawal_id: pallas::Base,
    pub nonce: u64,
}

impl AllocateCoverageV1CallData {
    pub fn new(
        pool_id: pallas::Base,
        member_public: PublicKey,
        coverage_amount: u64,
        withdrawal_id: pallas::Base,
        nonce: u64,
    ) -> Self {
        let (mx, my) = member_public.xy();
        Self {
            pool_id,
            member_pub_x: mx,
            member_pub_y: my,
            coverage_amount,
            withdrawal_id,
            nonce,
        }
    }

    pub fn compute_public_inputs(&self) -> AllocateCoverageV1PublicInputs {
        let derived_allocation_id = poseidon_hash([
            self.pool_id,
            self.member_pub_x,
            self.member_pub_y,
            pallas::Base::from(self.coverage_amount),
            self.withdrawal_id,
            pallas::Base::from(self.nonce),
        ]);
        AllocateCoverageV1PublicInputs {
            pool_id: self.pool_id,
            member_pub_x: self.member_pub_x,
            member_pub_y: self.member_pub_y,
            coverage_amount: pallas::Base::from(self.coverage_amount),
            withdrawal_id: self.withdrawal_id,
            nonce: pallas::Base::from(self.nonce),
            derived_allocation_id,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.pool_id)),
            Witness::Base(Value::known(self.member_pub_x)),
            Witness::Base(Value::known(self.member_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.coverage_amount))),
            Witness::Base(Value::known(self.withdrawal_id)),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
        ]
    }
}

/// Create an AllocateCoverage ZK proof
pub fn allocate_coverage_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &AllocateCoverageV1CallData,
) -> Result<(Proof, AllocateCoverageV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}