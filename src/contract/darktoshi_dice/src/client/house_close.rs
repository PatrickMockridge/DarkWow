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

//! DarkToshi Dice HouseCloseV1 Client API
//!
//! Proves that the house authorizes closing an abandoned bet.
//! Replaces Schnorr signature verification with a ZK proof.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{crypto::poseidon_hash, pasta::pallas};
use rand::rngs::OsRng;
use rand::SeedableRng;
use tracing::debug;

pub struct HouseClosePublicInputs {
    pub bet_id: pallas::Base,
    pub house_pub_x: pallas::Base,
    pub house_pub_y: pallas::Base,
    pub close_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl HouseClosePublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.bet_id,
            self.house_pub_x,
            self.house_pub_y,
            self.close_nullifier,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

pub struct HouseCloseCallData {
    pub bet_id: pallas::Base,
    pub house_secret: pallas::Base,
    pub house_pub_x: pallas::Base,
    pub house_pub_y: pallas::Base,
    pub close_nullifier: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl HouseCloseCallData {
    pub fn new() -> Self {
        Self {
            bet_id: pallas::Base::zero(),
            house_secret: pallas::Base::zero(),
            house_pub_x: pallas::Base::zero(),
            house_pub_y: pallas::Base::zero(),
            close_nullifier: pallas::Base::zero(),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> HouseClosePublicInputs {
        HouseClosePublicInputs {
            bet_id: self.bet_id,
            house_pub_x: self.house_pub_x,
            house_pub_y: self.house_pub_y,
            close_nullifier: self.close_nullifier,
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }
}

pub fn create_house_close_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    data: &HouseCloseCallData,
) -> Result<(Proof, HouseClosePublicInputs)> {
    debug!(target: "contract::dice::client::house_close", "Creating HouseCloseV1 ZK proof");

    let public_inputs = HouseClosePublicInputs {
        bet_id: data.bet_id,
        house_pub_x: data.house_pub_x,
        house_pub_y: data.house_pub_y,
        close_nullifier: data.close_nullifier,
        tx_binding: poseidon_hash([pallas::Base::from(3u64), data.tx_commitment, data.tx_nonce]),
        tx_nonce: data.tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(data.bet_id)),
        Witness::Base(Value::known(data.house_secret)),
        Witness::Base(Value::known(data.house_pub_x)),
        Witness::Base(Value::known(data.house_pub_y)),
        Witness::Base(Value::known(data.close_nullifier)),
        Witness::Base(Value::known(data.tx_commitment)),
        Witness::Base(Value::known(data.tx_nonce)),
        Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), data.tx_commitment, data.tx_nonce]))), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
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
