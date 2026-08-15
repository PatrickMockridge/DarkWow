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

//! Relayer Endowment initialize_v1 ZK proof generation

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

/// InitializeV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct InitializeV1PublicInputs {
    pub endowment_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl InitializeV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.endowment_id, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for initialize proof generation
#[derive(Debug, Clone)]
pub struct InitializeV1CallData {
    pub relayer_pub_x: pallas::Base,
    pub relayer_pub_y: pallas::Base,
    pub config_hash: pallas::Base,
    pub nonce: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl InitializeV1CallData {
    pub fn new(relayer_public: PublicKey, default_backer_cut_bp: u32, nonce: u64) -> Self {
        let (px, py) = relayer_public.xy().expect("pk not identity");
        let config_hash = poseidon_hash([pallas::Base::from(default_backer_cut_bp as u64)]);
        Self { relayer_pub_x: px, relayer_pub_y: py, config_hash, nonce: pallas::Base::from(nonce), tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    pub fn compute_public_inputs(&self) -> InitializeV1PublicInputs {
        let endowment_id = poseidon_hash([
            self.relayer_pub_x,
            self.relayer_pub_y,
            self.config_hash,
            self.nonce,
        ]);
        InitializeV1PublicInputs { endowment_id, tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]), tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.relayer_pub_x)),
            Witness::Base(Value::known(self.relayer_pub_y)),
            Witness::Base(Value::known(self.config_hash)),
            Witness::Base(Value::known(self.nonce)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))), // tx_binding
        ]
    }
}

/// Create a Initialize ZK proof
pub fn initialize_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &InitializeV1CallData,
) -> Result<(Proof, InitializeV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
