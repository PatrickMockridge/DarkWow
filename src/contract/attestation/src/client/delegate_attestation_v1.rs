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

//! Attestation delegate_attestation_v1 ZK proof generation

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

/// DelegateAttestationV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct DelegateAttestationV1PublicInputs {
    pub delegation_id: pallas::Base,
    pub parent_id: pallas::Base,
    pub delegator_pub_x: pallas::Base,
    pub delegator_pub_y: pallas::Base,
    pub delegatee_pub_x: pallas::Base,
    pub delegatee_pub_y: pallas::Base,
    pub delegation_type: pallas::Base,
    pub max_ratio: pallas::Base,
    pub revocation_root: pallas::Base,
    pub chain_root: pallas::Base,
    pub current_depth: pallas::Base,
    pub max_depth: pallas::Base,
    pub delegator_stake: pallas::Base,
    pub delegatee_stake: pallas::Base,
}

impl DelegateAttestationV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.delegation_id,
            self.parent_id,
            self.delegator_pub_x,
            self.delegator_pub_y,
            self.delegatee_pub_x,
            self.delegatee_pub_y,
            self.delegation_type,
            self.max_ratio,
            self.revocation_root,
            self.chain_root,
            self.current_depth,
            self.max_depth,
            self.delegator_stake,
            self.delegatee_stake,
        ]
    }
}

/// Input data for delegate_attestation proof generation
#[derive(Debug, Clone)]
pub struct DelegateAttestationV1CallData {
    pub delegation_id: pallas::Base,
    pub parent_id: pallas::Base,
    pub delegator_secret: pallas::Base,
    pub delegation_type: pallas::Base,
    pub max_ratio: pallas::Base,
    pub revocation_root: pallas::Base,
    pub chain_root: pallas::Base,
    pub current_depth: pallas::Base,
    pub max_depth: pallas::Base,
    pub delegator_stake: pallas::Base,
    pub delegatee_stake: pallas::Base,
    pub nonce: pallas::Base,
    pub pos: u64,
    pub path: Vec<pallas::Base>,
    pub chain_pos: u64,
    pub chain_path: Vec<pallas::Base>,
    // Public inputs
    pub delegator_public: PublicKey,
    pub delegatee_public: PublicKey,
}

impl DelegateAttestationV1CallData {
    pub fn new(
        delegation_id: pallas::Base,
        parent_id: pallas::Base,
        delegator_secret: pallas::Base,
        delegation_type: pallas::Base,
        max_ratio: pallas::Base,
        revocation_root: pallas::Base,
        chain_root: pallas::Base,
        current_depth: pallas::Base,
        max_depth: pallas::Base,
        delegator_stake: pallas::Base,
        delegatee_stake: pallas::Base,
        nonce: pallas::Base,
        pos: u64,
        path: Vec<pallas::Base>,
        chain_pos: u64,
        chain_path: Vec<pallas::Base>,
        delegator_public: PublicKey,
        delegatee_public: PublicKey,
    ) -> Self {
        Self {
            delegation_id,
            parent_id,
            delegator_secret,
            delegation_type,
            max_ratio,
            revocation_root,
            chain_root,
            current_depth,
            max_depth,
            delegator_stake,
            delegatee_stake,
            nonce,
            pos,
            path,
            chain_pos,
            chain_path,
            delegator_public,
            delegatee_public,
        }
    }

    pub fn compute_public_inputs(&self) -> DelegateAttestationV1PublicInputs {
        let (dx, dy) = self.delegator_public.xy();
        let (ex, ey) = self.delegatee_public.xy();
        DelegateAttestationV1PublicInputs {
            delegation_id: self.delegation_id,
            parent_id: self.parent_id,
            delegator_pub_x: dx,
            delegator_pub_y: dy,
            delegatee_pub_x: ex,
            delegatee_pub_y: ey,
            delegation_type: self.delegation_type,
            max_ratio: self.max_ratio,
            revocation_root: self.revocation_root,
            chain_root: self.chain_root,
            current_depth: self.current_depth,
            max_depth: self.max_depth,
            delegator_stake: self.delegator_stake,
            delegatee_stake: self.delegatee_stake,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (dx, dy) = self.delegator_public.xy();
        let (ex, ey) = self.delegatee_public.xy();
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.delegation_id)),
            Witness::Base(Value::known(self.parent_id)),
            Witness::Base(Value::known(dx)),
            Witness::Base(Value::known(dy)),
            Witness::Base(Value::known(ex)),
            Witness::Base(Value::known(ey)),
            Witness::Base(Value::known(self.delegation_type)),
            Witness::Base(Value::known(self.max_ratio)),
            Witness::Base(Value::known(self.revocation_root)),
            Witness::Base(Value::known(self.chain_root)),
            Witness::Base(Value::known(self.current_depth)),
            Witness::Base(Value::known(self.max_depth)),
            Witness::Base(Value::known(self.delegator_stake)),
            Witness::Base(Value::known(self.delegatee_stake)),
            // Private inputs
            Witness::Base(Value::known(self.nonce)),
            Witness::Uint64(Value::known(self.pos)),
            Witness::MerklePath(Value::known(self.path.iter().map(|&v| darkfi_sdk::crypto::MerkleNode::new(v)).collect::<Vec<_>>().try_into().unwrap())),
            Witness::Uint64(Value::known(self.chain_pos)),
            Witness::MerklePath(Value::known(self.chain_path.iter().map(|&v| darkfi_sdk::crypto::MerkleNode::new(v)).collect::<Vec<_>>().try_into().unwrap())),
            Witness::Base(Value::known(self.delegator_secret)),
        ]
    }
}

/// Create a DelegateAttestation ZK proof
pub fn delegate_attestation_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &DelegateAttestationV1CallData,
) -> Result<(Proof, DelegateAttestationV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}