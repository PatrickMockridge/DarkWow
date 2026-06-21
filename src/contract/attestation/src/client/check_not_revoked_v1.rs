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

//! Attestation check_not_revoked_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, MerkleNode},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// CheckNotRevokedV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CheckNotRevokedV1PublicInputs {
    pub revocation_root: pallas::Base,
    pub nonce: pallas::Base,
    pub tx_commitment: pallas::Base,
}

impl CheckNotRevokedV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.revocation_root, self.nonce, self.tx_commitment]
    }
}

/// Input data for check_not_revoked proof generation
#[derive(Debug, Clone)]
pub struct CheckNotRevokedV1CallData {
    pub revocation_root: pallas::Base,
    pub nonce: pallas::Base,
    pub pos: u64,
    pub path: Vec<MerkleNode>,
    pub tx_commitment: pallas::Base,
}

impl CheckNotRevokedV1CallData {
    pub fn new(revocation_root: pallas::Base, nonce: pallas::Base, pos: u64, path: Vec<MerkleNode>) -> Self {
        Self { revocation_root, nonce, pos, path, tx_commitment: pallas::Base::zero() }
    }

    /// Compute leaf from nonce
    pub fn compute_leaf(&self) -> pallas::Base {
        poseidon_hash([self.nonce])
    }

    pub fn compute_public_inputs(&self) -> CheckNotRevokedV1PublicInputs {
        CheckNotRevokedV1PublicInputs { revocation_root: self.revocation_root, nonce: self.nonce, tx_commitment: self.tx_commitment }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.revocation_root)),
            Witness::Base(Value::known(self.nonce)),
            // Private inputs
            Witness::Uint64(Value::known(self.pos)),
            Witness::MerklePath(Value::known(self.path.clone().try_into().unwrap())),
            Witness::Base(Value::known(self.compute_leaf())),
        ]
    }
}

/// Create a CheckNotRevoked ZK proof
pub fn check_not_revoked_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CheckNotRevokedV1CallData,
) -> Result<(Proof, CheckNotRevokedV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}