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

//! Attestation verify_chain_v1 ZK proof generation (V2 circuit)

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

/// VerifyChainV1 circuit public inputs (V2: only tx_binding, tx_nonce)
#[derive(Debug, Clone)]
pub struct VerifyChainV1PublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl VerifyChainV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce]
    }
}

pub struct VerifyChainV1CallData {
    pub chain_root: pallas::Base,
    pub verifier_secret: pallas::Base,
    pub verifier_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl VerifyChainV1CallData {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _delegation_id: pallas::Base,
        _parent_id: pallas::Base,
        chain_root: pallas::Base,
        _current_depth: pallas::Base,
        _max_depth: pallas::Base,
        _pos: pallas::Base,
        _path: [pallas::Base; 255],
    ) -> Self {
        Self {
            chain_root,
            verifier_secret: pallas::Base::zero(),
            verifier_public: PublicKey::from_secret(
                dwow_sdk::crypto::SecretKey::from_base(pallas::Base::from(1u64)),
            ),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> VerifyChainV1PublicInputs {
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        VerifyChainV1PublicInputs { tx_binding, tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (vx, vy) = self.verifier_public.xy().expect("pk not identity");
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        vec![
            Witness::Base(Value::known(self.chain_root)),
            Witness::Base(Value::known(self.verifier_secret)),
            Witness::Base(Value::known(vx)),
            Witness::Base(Value::known(vy)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ]
    }
}

pub fn verify_chain_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &VerifyChainV1CallData,
) -> Result<(Proof, VerifyChainV1PublicInputs)> {
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
