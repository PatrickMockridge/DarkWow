/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 * ...license header...
 */

//! Attestation update_delegation_v1 ZK proof generation (V2 circuit)

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

/// UpdateDelegationV1 circuit public inputs (V2: only tx_binding, tx_nonce)
#[derive(Debug, Clone)]
pub struct UpdateDelegationV1PublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UpdateDelegationV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce]
    }
}

/// Input data for update_delegation proof generation
#[derive(Debug, Clone)]
pub struct UpdateDelegationV1CallData {
    pub delegator_secret: pallas::Base,
    pub delegator_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UpdateDelegationV1CallData {
    pub fn new(
        _original_attestation_id: pallas::Base,
        _delegation_type: pallas::Base,
        _current_depth: pallas::Base,
        _max_depth: pallas::Base,
        _delegator_stake: pallas::Base,
        _delegatee_stake: pallas::Base,
        _max_ratio: pallas::Base,
    ) -> Self {
        Self {
            delegator_secret: pallas::Base::zero(),
            delegator_public: PublicKey::from_secret(
                dwow_sdk::crypto::SecretKey::from_base(pallas::Base::from(1u64)),
            ),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> UpdateDelegationV1PublicInputs {
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        UpdateDelegationV1PublicInputs { tx_binding, tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (dx, dy) = self.delegator_public.xy().expect("pk not identity");
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        vec![
            Witness::Base(Value::known(self.delegator_secret)),
            Witness::Base(Value::known(dx)),
            Witness::Base(Value::known(dy)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ]
    }
}

pub fn update_delegation_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &UpdateDelegationV1CallData,
) -> Result<(Proof, UpdateDelegationV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();
    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;
    Ok((proof, public_inputs))
}
