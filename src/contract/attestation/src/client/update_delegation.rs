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
use rand::SeedableRng;

/// UpdateDelegationV1 circuit public inputs (V2: tx_binding, tx_nonce)
///
/// OBL-C196(ii): the delegator's coordinates are back. `delegator_pub` is now on the wire and
/// `update_delegation_v1` requires it to be the original attestation's `attestor_pub`, so the
/// circuit derive-and-exposes the coordinates and the arm publishes them.
#[derive(Debug, Clone)]
pub struct UpdateDelegationV1PublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub delegator_pub_x: pallas::Base,
    pub delegator_pub_y: pallas::Base,
}

impl UpdateDelegationV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // `OBL-C198`: the tx pair is the last two instances (matching the reordered circuit).
        vec![
            self.delegator_pub_x,
            self.delegator_pub_y,
            self.tx_binding,
            self.tx_nonce,
        ]
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
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (dx, dy) = self.delegator_public.xy().expect("pk not identity");
        // Circuit: DOMAIN_TX_BINDING = witness_base(3) = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        UpdateDelegationV1PublicInputs {
            tx_binding,
            tx_nonce: self.tx_nonce,
            delegator_pub_x: dx,
            delegator_pub_y: dy,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        // Circuit witness order: tx_commitment, tx_nonce, tx_binding,
        // delegator_secret, delegator_pub_x, delegator_pub_y
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (dx, dy) = self.delegator_public.xy().expect("pk not identity");
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        vec![
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
            Witness::Base(Value::known(self.delegator_secret)),
            Witness::Base(Value::known(dx)),
            Witness::Base(Value::known(dy)),
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
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    Ok((proof, public_inputs))
}
