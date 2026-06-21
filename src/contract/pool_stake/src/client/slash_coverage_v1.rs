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

//! Pool Stake SlashCoverage ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// SlashCoverageV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SlashCoverageV1PublicInputs {
    pub allocation_id: pallas::Base,
    pub slashed_amount: pallas::Base,
    pub slashed_to_pub_x: pallas::Base,
    pub slashed_to_pub_y: pallas::Base,
    pub nonce: pallas::Base,
    pub derived_slash_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SlashCoverageV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // Only constrain_instance values (derived_slash_id is the sole public instance)
        vec![self.derived_slash_id, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for SlashCoverage proof generation
#[derive(Debug, Clone)]
pub struct SlashCoverageV1CallData {
    pub allocation_id: pallas::Base,
    pub slashed_amount: u64,
    pub slashed_to_pub_x: pallas::Base,
    pub slashed_to_pub_y: pallas::Base,
    pub nonce: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SlashCoverageV1CallData {
    pub fn new(
        allocation_id: pallas::Base,
        slashed_amount: u64,
        slashed_to_public: PublicKey,
        nonce: u64,
    ) -> Self {
        let (sx, sy) = slashed_to_public.xy();
        Self {
            allocation_id,
            slashed_amount,
            slashed_to_pub_x: sx,
            slashed_to_pub_y: sy,
            nonce,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> SlashCoverageV1PublicInputs {
        let derived_slash_id = poseidon_hash([
            self.allocation_id,
            pallas::Base::from(self.slashed_amount),
            self.slashed_to_pub_x,
            self.slashed_to_pub_y,
            pallas::Base::from(self.nonce),
        ]);
        SlashCoverageV1PublicInputs {
            allocation_id: self.allocation_id,
            slashed_amount: pallas::Base::from(self.slashed_amount),
            slashed_to_pub_x: self.slashed_to_pub_x,
            slashed_to_pub_y: self.slashed_to_pub_y,
            nonce: pallas::Base::from(self.nonce),
            derived_slash_id,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.allocation_id)),
            Witness::Base(Value::known(pallas::Base::from(self.slashed_amount))),
            Witness::Base(Value::known(self.slashed_to_pub_x)),
            Witness::Base(Value::known(self.slashed_to_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a SlashCoverage ZK proof
pub fn slash_coverage_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SlashCoverageV1CallData,
) -> Result<(Proof, SlashCoverageV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}