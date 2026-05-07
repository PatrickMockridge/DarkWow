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

//! Escrow fund_v1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    bridgetree::Hashable,
    crypto::{pedersen_commitment_u64, pasta_prelude::Curve, pasta_prelude::CurveAffine, Blind, MerkleNode},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// FundEscrowV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct FundEscrowPublicInputs {
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
    pub escrow_id: pallas::Base,
    pub merkle_root: pallas::Base,
}

impl FundEscrowPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.value_commit_x,
            self.value_commit_y,
            self.escrow_id,
            self.merkle_root,
        ]
    }
}

/// Input data for fund_escrow proof generation
#[derive(Debug, Clone)]
pub struct FundEscrowCallData {
    pub value: u64,
    pub value_blind: pallas::Scalar,
    pub escrow_id: pallas::Base,
    pub merkle_leaf_pos: u32,
    pub merkle_path: Vec<MerkleNode>,
}

impl FundEscrowCallData {
    pub fn new(
        value: u64,
        value_blind: pallas::Scalar,
        escrow_id: pallas::Base,
        merkle_leaf_pos: u32,
        merkle_path: Vec<MerkleNode>,
    ) -> Self {
        Self { value, value_blind, escrow_id, merkle_leaf_pos, merkle_path }
    }

    /// Compute merkle root from leaf position and path
    pub fn compute_merkle_root(&self) -> pallas::Base {
        let mut current = MerkleNode::new(self.escrow_id);
        let position: u64 = self.merkle_leaf_pos.into();
        for (level, sibling) in self.merkle_path.iter().enumerate() {
            let level = level as u8;
            current = if position & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, sibling)
            } else {
                MerkleNode::combine(level.into(), sibling, &current)
            };
        }
        current.inner()
    }

    pub fn compute_public_inputs(&self) -> FundEscrowPublicInputs {
        // Compute actual Pedersen commitment for value to match circuit behavior
        let value_commit = pedersen_commitment_u64(self.value, Blind(self.value_blind));
        let value_coords = value_commit.to_affine().coordinates().unwrap();

        FundEscrowPublicInputs {
            // For the circuit, we use the computed commitment coordinates
            // The circuit constrains these via ec_mul_short and ec_add
            value_commit_x: *value_coords.x(),
            value_commit_y: *value_coords.y(),
            escrow_id: self.escrow_id,
            merkle_root: self.compute_merkle_root(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Witnesses must match circuit order (fund_v1.zk witness block):
            // 1. Base escrow_id
            Witness::Base(Value::known(self.escrow_id)),
            // 2. Base value
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            // 3. Scalar value_blind
            Witness::Scalar(Value::known(self.value_blind)),
            // 4. Uint32 merkle_leaf_pos
            Witness::Uint32(Value::known(self.merkle_leaf_pos)),
            // 5. MerklePath merkle_path
            Witness::MerklePath(Value::known(self.merkle_path.clone().try_into().unwrap())),
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