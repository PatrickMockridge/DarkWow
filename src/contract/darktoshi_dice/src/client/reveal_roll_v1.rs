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

//! DarkToshi Dice RevealRollV1 Client API
//!
//! Proves knowledge of the secret nonce used during bet commitment.
//! Replaces the plaintext `secret_nonce` comparison with a ZK proof.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::pasta::pallas;
use rand::rngs::OsRng;
use tracing::debug;

pub struct RevealRollPublicInputs {
    pub bet_id: pallas::Base,
    pub secret_nonce_commit: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevealRollPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.bet_id, self.secret_nonce_commit, self.tx_binding,
            self.tx_nonce]
    }
}

pub struct RevealRollCallData {
    pub bet_id: pallas::Base,
    pub secret_nonce: pallas::Base,
    pub secret_nonce_commit: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevealRollCallData {
    pub fn new() -> Self {
        Self {
            bet_id: pallas::Base::zero(),
            secret_nonce: pallas::Base::zero(),
            secret_nonce_commit: pallas::Base::zero(),
            tx_commitment: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> RevealRollPublicInputs {
        RevealRollPublicInputs {
            bet_id: self.bet_id,
            secret_nonce_commit: self.secret_nonce_commit,
            tx_binding: poseidon_hash([self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }
}

pub fn create_reveal_roll_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    data: &RevealRollCallData,
) -> Result<(Proof, RevealRollPublicInputs)> {
    debug!(target: "contract::dice::client::reveal_roll", "Creating RevealRollV1 ZK proof");

    let public_inputs = RevealRollPublicInputs {
        bet_id: data.bet_id,
        secret_nonce_commit: data.secret_nonce_commit,
        tx_binding: poseidon_hash([data.tx_commitment, data.tx_nonce]),
            tx_nonce: data.tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(data.bet_id)),
        Witness::Base(Value::known(data.secret_nonce)),
        Witness::Base(Value::known(data.secret_nonce_commit)),
        Witness::Base(Value::known(data.tx_commitment)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
