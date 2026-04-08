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

//! Escrow fund_v1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{pasta::pallas, crypto::MerkleNode};
use rand::rngs::OsRng;

/// FundEscrowV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct FundEscrowPublicInputs {
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
    pub escrow_id: pallas::Base,
}

impl FundEscrowPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.value_commit_x, self.value_commit_y, self.escrow_id]
    }
}

/// Input data for fund_escrow proof generation
#[derive(Debug, Clone)]
pub struct FundEscrowCallData {
    pub value: u64,
    pub value_blind: pallas::Base,
    pub escrow_id: pallas::Base,
    pub merkle_sibling_0: pallas::Base,
    pub merkle_sibling_1: pallas::Base,
}

impl FundEscrowCallData {
    pub fn new(
        value: u64,
        value_blind: pallas::Base,
        escrow_id: pallas::Base,
        merkle_sibling_0: pallas::Base,
        merkle_sibling_1: pallas::Base,
    ) -> Self {
        Self {
            value,
            value_blind,
            escrow_id,
            merkle_sibling_0,
            merkle_sibling_1,
        }
    }

    pub fn compute_public_inputs(&self) -> FundEscrowPublicInputs {
        // Value commitment is computed in circuit from witnesses
        // Public inputs are the coordinates and escrow_id
        FundEscrowPublicInputs {
            value_commit_x: pallas::Base::zero(), // Will be computed by circuit
            value_commit_y: pallas::Base::zero(), // Will be computed by circuit
            escrow_id: self.escrow_id,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(pallas::Base::zero())), // value_commit_x (computed in circuit)
            Witness::Base(Value::known(pallas::Base::zero())), // value_commit_y (computed in circuit)
            Witness::Base(Value::known(self.escrow_id)),
            // Private inputs
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Base(Value::known(self.value_blind)),
            Witness::Base(Value::known(pallas::Base::from(3))), // merkle_path_length = 3
            Witness::Base(Value::known(self.merkle_sibling_0)),
            Witness::Base(Value::known(self.merkle_sibling_1)),
        ]
    }
}

/// Create a FundEscrow ZK proof
pub fn create_fund_escrow_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &FundEscrowCallData,
) -> Result<(Proof, FundEscrowPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}