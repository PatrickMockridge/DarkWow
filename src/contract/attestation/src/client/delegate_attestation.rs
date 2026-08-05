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

//! Attestation delegate_attestation_v1 ZK proof generation (V2 circuit)

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

/// DelegateAttestationV1 circuit public inputs (V2: delegatee_leaf, tx_binding, tx_nonce)
#[derive(Debug, Clone)]
pub struct DelegateAttestationV1PublicInputs {
    pub delegatee_leaf: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DelegateAttestationV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.delegatee_leaf, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for delegate_attestation proof generation
#[derive(Debug, Clone)]
pub struct DelegateAttestationV1CallData {
    pub delegator_secret: pallas::Base,
    pub delegatee_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DelegateAttestationV1CallData {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _delegation_id: pallas::Base,
        _parent_id: pallas::Base,
        delegator_secret: pallas::Base,
        _delegation_type: pallas::Base,
        _max_ratio: pallas::Base,
        _revocation_root: pallas::Base,
        _chain_root: pallas::Base,
        _current_depth: pallas::Base,
        _max_depth: pallas::Base,
        _delegator_stake: pallas::Base,
        _delegatee_stake: pallas::Base,
        _nonce: pallas::Base,
        _pos: pallas::Base,
        _path: [pallas::Base; 255],
        _chain_pos: pallas::Base,
        _chain_path: [pallas::Base; 255],
        _delegator_public: PublicKey,
        delegatee_public: PublicKey,
    ) -> Self {
        Self {
            delegator_secret,
            delegatee_public,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> DelegateAttestationV1PublicInputs {
        let (ex, ey) = self.delegatee_public.xy().expect("pk not identity");
        // DOMAIN_COIN_COMMIT = witness_base(4) = 4
        let delegatee_leaf = poseidon_hash([pallas::Base::from(4u64), ex, ey]);
        // DOMAIN_TX_BINDING = witness_base(3) = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        DelegateAttestationV1PublicInputs { delegatee_leaf, tx_binding, tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        // Circuit witness order: delegatee_pub_x, delegatee_pub_y, delegator_secret, tx_commitment, tx_nonce, tx_binding
        let (ex, ey) = self.delegatee_public.xy().expect("pk not identity");
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        vec![
            Witness::Base(Value::known(ex)),
            Witness::Base(Value::known(ey)),
            Witness::Base(Value::known(self.delegator_secret)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
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
