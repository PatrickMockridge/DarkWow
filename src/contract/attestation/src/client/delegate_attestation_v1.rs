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

//! Attestation delegate_attestation_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::PublicKey,
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
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DelegateAttestationV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // Public input order must match constrain_instance execution order:
        // Phase 3: set_membership internally calls constrain_instance(chain_root)   [0]
        // Phase 5: set_membership internally calls constrain_instance(revocation_root) [1]
        // Phase 6: explicit constrain_instance calls:
        vec![
            // Implicit from set_membership ops
            self.chain_root,
            self.revocation_root,
            // Explicit
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
            self.tx_binding,
            self.tx_nonce,
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
    pub pos: pallas::Base,
    pub path: [pallas::Base; 255],
    pub chain_pos: pallas::Base,
    pub chain_path: [pallas::Base; 255],
    // Public inputs
    pub delegator_public: PublicKey,
    pub delegatee_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DelegateAttestationV1CallData {
    #[allow(clippy::too_many_arguments)]
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
        pos: pallas::Base,
        path: [pallas::Base; 255],
        chain_pos: pallas::Base,
        chain_path: [pallas::Base; 255],
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
            tx_commitment: pallas::Base::zero(),
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
            tx_binding: poseidon_hash([self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (dx, dy) = self.delegator_public.xy();
        let (ex, ey) = self.delegatee_public.xy();
        vec![
            // Public inputs (indices 0-13)
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
            // Private witnesses (indices 14-19)
            Witness::Base(Value::known(self.nonce)),
            Witness::Base(Value::known(self.pos)),
            Witness::SparseMerklePath(Value::known(self.path)),
            Witness::Base(Value::known(self.chain_pos)),
            Witness::SparseMerklePath(Value::known(self.chain_path)),
            Witness::Base(Value::known(self.delegator_secret)),
            // Delegation type constants (indices 20-22)
            Witness::Base(Value::known(pallas::Base::from(0u64))),
            Witness::Base(Value::known(pallas::Base::from(1u64))),
            Witness::Base(Value::known(pallas::Base::from(2u64))),
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
