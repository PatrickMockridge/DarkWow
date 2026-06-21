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

//! Attestation verify_chain_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{crypto::MerkleNode, pasta::pallas};
use rand::rngs::OsRng;

/// VerifyChainV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct VerifyChainV1PublicInputs {
    pub delegation_id: pallas::Base,
    pub parent_id: pallas::Base,
    pub chain_root: pallas::Base,
    pub current_depth: pallas::Base,
    pub max_depth: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl VerifyChainV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.delegation_id,
            self.parent_id,
            self.chain_root,
            self.current_depth,
            self.max_depth,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for verify_chain proof generation
#[derive(Debug, Clone)]
pub struct VerifyChainV1CallData {
    pub delegation_id: pallas::Base,
    pub parent_id: pallas::Base,
    pub chain_root: pallas::Base,
    pub current_depth: pallas::Base,
    pub max_depth: pallas::Base,
    pub pos: u64,
    pub path: Vec<MerkleNode>,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl VerifyChainV1CallData {
    pub fn new(
        delegation_id: pallas::Base,
        parent_id: pallas::Base,
        chain_root: pallas::Base,
        current_depth: pallas::Base,
        max_depth: pallas::Base,
        pos: u64,
        path: Vec<MerkleNode>,
    ) -> Self {
        Self {
            delegation_id,
            parent_id,
            chain_root,
            current_depth,
            max_depth,
            pos,
            path,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> VerifyChainV1PublicInputs {
        VerifyChainV1PublicInputs {
            delegation_id: self.delegation_id,
            parent_id: self.parent_id,
            chain_root: self.chain_root,
            current_depth: self.current_depth,
            max_depth: self.max_depth,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.delegation_id)),
            Witness::Base(Value::known(self.parent_id)),
            Witness::Base(Value::known(self.chain_root)),
            Witness::Base(Value::known(self.current_depth)),
            Witness::Base(Value::known(self.max_depth)),
            // Private inputs
            Witness::Uint64(Value::known(self.pos)),
            Witness::MerklePath(Value::known(self.path.clone().try_into().unwrap())),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a VerifyChain ZK proof
pub fn verify_chain_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &VerifyChainV1CallData,
) -> Result<(Proof, VerifyChainV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}