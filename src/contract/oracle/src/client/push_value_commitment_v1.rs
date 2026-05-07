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

//! Oracle push_value_commitment_v1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, MerkleNode, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// PushValueCommitmentV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct PushValueCommitmentV1PublicInputs {
    pub oracle_id: pallas::Base,
    pub commitment: pallas::Base,
    pub data_root: pallas::Base,
}

impl PushValueCommitmentV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.oracle_id, self.commitment, self.data_root]
    }
}

/// Input data for push_value_commitment proof generation
#[derive(Debug, Clone)]
pub struct PushValueCommitmentV1CallData {
    pub oracle_id: pallas::Base,
    pub staker_secret: pallas::Base,
    pub pos: u64,
    pub path: Vec<MerkleNode>,
    pub value: pallas::Base,
    pub nonce: pallas::Base,
    // Public inputs
    pub staker_public: PublicKey,
    pub commitment: pallas::Base,
    pub data_root: pallas::Base,
}

impl PushValueCommitmentV1CallData {
    pub fn new(
        oracle_id: pallas::Base,
        staker_secret: pallas::Base,
        pos: u64,
        path: Vec<MerkleNode>,
        value: pallas::Base,
        nonce: pallas::Base,
        staker_public: PublicKey,
        commitment: pallas::Base,
        data_root: pallas::Base,
    ) -> Self {
        Self {
            oracle_id,
            staker_secret,
            pos,
            path,
            value,
            nonce,
            staker_public,
            commitment,
            data_root,
        }
    }

    /// Compute commitment from value and nonce
    pub fn compute_commitment(&self) -> pallas::Base {
        poseidon_hash([self.value, self.nonce])
    }

    pub fn compute_public_inputs(&self) -> PushValueCommitmentV1PublicInputs {
        PushValueCommitmentV1PublicInputs {
            oracle_id: self.oracle_id,
            commitment: self.commitment,
            data_root: self.data_root,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.staker_public.xy();
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.oracle_id)),
            Witness::Base(Value::known(self.commitment)),
            Witness::Base(Value::known(self.data_root)),
            // Private inputs
            Witness::Base(Value::known(self.staker_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Uint64(Value::known(self.pos)),
            Witness::MerklePath(Value::known(self.path.clone().try_into().unwrap())),
            Witness::Base(Value::known(self.value)),
            Witness::Base(Value::known(self.nonce)),
        ]
    }
}

/// Create a PushValueCommitment ZK proof
pub fn push_value_commitment_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PushValueCommitmentV1CallData,
) -> Result<(Proof, PushValueCommitmentV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}