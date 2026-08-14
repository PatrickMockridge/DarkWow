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

//! Slot RevealSpinV1 Client API
//!
//! Proves knowledge of the secret nonce used during spin commitment.
//! Replaces the plaintext `secret_nonce` comparison with a ZK proof.
//!
//! The player proves they know `secret_nonce` such that:
//! `poseidon_hash(secret_nonce) == stored_nonce_commit`

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::pasta::pallas;
use rand::rngs::OsRng;
use rand::SeedableRng;
use tracing::debug;

/// Public inputs for RevealSpinV1 ZK proof.
/// Order must match RevealSpin_V1 circuit:
/// spin_id, secret_nonce_commit, tx_commitment
pub struct RevealSpinPublicInputs {
    pub spin_id: pallas::Base,
    pub secret_nonce_commit: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevealSpinPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.spin_id, self.secret_nonce_commit, self.tx_binding, self.tx_nonce]
    }
}

/// Data required to create a RevealSpinV1 proof.
pub struct RevealSpinCallData {
    pub spin_id: pallas::Base,
    pub secret_nonce: pallas::Base,
    pub secret_nonce_commit: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevealSpinCallData {
    pub fn new() -> Self {
        Self {
            spin_id: pallas::Base::zero(),
            secret_nonce: pallas::Base::zero(),
            secret_nonce_commit: pallas::Base::zero(),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> RevealSpinPublicInputs {
        RevealSpinPublicInputs {
            spin_id: self.spin_id,
            secret_nonce_commit: self.secret_nonce_commit,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }
}

/// Create a RevealSpinV1 ZK proof.
///
/// Witness order must match RevealSpin_V1 circuit:
/// spin_id, secret_nonce, secret_nonce_commit, tx_commitment
pub fn create_reveal_spin_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    data: &RevealSpinCallData,
) -> Result<(Proof, RevealSpinPublicInputs)> {
    debug!(target: "contract::slot::client::reveal_spin", "Creating RevealSpinV1 ZK proof");

    let public_inputs = RevealSpinPublicInputs {
        spin_id: data.spin_id,
        secret_nonce_commit: data.secret_nonce_commit,
        tx_binding: pallas::Base::zero(),
        tx_nonce: data.tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(data.spin_id)),
        Witness::Base(Value::known(data.secret_nonce)),
        Witness::Base(Value::known(data.secret_nonce_commit)),
        Witness::Base(Value::known(data.tx_commitment)),
        Witness::Base(Value::known(data.tx_nonce)),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
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

/// Builder for `Slot::RevealSpinV1` contract call.
pub struct RevealSpinCallBuilder {
    pub call_data: RevealSpinCallData,
    pub zkbin: ZkBinary,
    pub pk: ProvingKey,
}

impl RevealSpinCallBuilder {
    pub fn build(self) -> Result<RevealSpinCallDebris> {
        debug!(target: "contract::slot::client::reveal_spin", "Building Slot::RevealSpinV1 contract call");

        let (proof, public_inputs) = create_reveal_spin_proof(
            &self.zkbin,
            &self.pk,
            &self.call_data,
        )?;

        Ok(RevealSpinCallDebris {
            proof,
            public_inputs,
        })
    }
}

/// Debris produced by building a RevealSpin call.
pub struct RevealSpinCallDebris {
    pub proof: Proof,
    pub public_inputs: RevealSpinPublicInputs,
}
