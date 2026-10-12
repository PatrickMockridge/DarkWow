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

//! UpdateConfig ZK proof generation
//!
//! `UpdateConfigV2` witnesses the governance identity and publishes the same five values the
//! entrypoint's arm publishes, in the same order (`entrypoint/update_config.rs`):
//! `[gov_pub_x, gov_pub_y, gov_nullifier, tx_binding, tx_nonce]`.
//!
//! The governance point is the **identity point** here — `gov_secret = 0`, so `ec_mul_base(0, K)`
//! is `(0, 0)`. That is sound for this endpoint and is the same assignment the stablecoin harness
//! uses for its byte-identical `UpdateConfigV2` circuit: nothing compares `gov_pub_x/y` to a stored
//! key. Authorization is the nullifier's absence from the nullifiers tree, and the circuit is what
//! binds the point to the secret that produced it.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::poseidon_hash,
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

use crate::model::UpdateConfigParams;

/// `UpdateConfigV2` public inputs, in the order of its `constrain_instance` calls.
#[derive(Debug, Clone)]
pub struct UpdateConfigPublicInputs {
    pub gov_pub_x: pallas::Base,
    pub gov_pub_y: pallas::Base,
    /// `poseidon_hash([1, gov_pub_x, gov_pub_y, gov_secret])`
    pub gov_nullifier: pallas::Base,
    /// `poseidon_hash([3, tx_commitment, tx_nonce])`
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UpdateConfigPublicInputs {
    /// Convert to vector for ZK proof creation.
    /// Order must match `constrain_instance` calls in `proof/update_config.zk`.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.gov_pub_x, self.gov_pub_y, self.gov_nullifier, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for `UpdateConfig` proof generation.
#[derive(Debug, Clone)]
pub struct UpdateConfigCallData {
    pub timeout: u32,
    pub fee: u64,
    /// The governance secret. Zero here — see the module note.
    pub gov_secret: pallas::Base,
    pub gov_pub_x: pallas::Base,
    pub gov_pub_y: pallas::Base,
    pub gov_nullifier: pallas::Base,
    /// The commitment over the whole call set, set by the caller once the calls are assembled.
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UpdateConfigCallData {
    /// Create new call data at the identity-point governance assignment.
    pub fn new(timeout: u32, fee: u64) -> Self {
        let z = pallas::Base::zero();
        Self {
            timeout,
            fee,
            gov_secret: z,
            gov_pub_x: z,
            gov_pub_y: z,
            gov_nullifier: poseidon_hash([pallas::Base::from(1u64), z, z, z]),
            tx_commitment: z,
            tx_nonce: z,
        }
    }

    /// The params the contract's own encoder must be able to read back — the arm decodes
    /// `UpdateConfigParams` from `call_data[1..]`, and publishes these same values.
    pub fn params(&self) -> UpdateConfigParams {
        UpdateConfigParams {
            timeout: self.timeout,
            fee: self.fee,
            gov_pub_x: self.gov_pub_x,
            gov_pub_y: self.gov_pub_y,
            gov_nullifier: self.gov_nullifier,
            tx_nonce: self.tx_nonce,
        }
    }

    /// Compute public inputs for this call.
    pub fn compute_public_inputs(&self) -> UpdateConfigPublicInputs {
        UpdateConfigPublicInputs {
            gov_pub_x: self.gov_pub_x,
            gov_pub_y: self.gov_pub_y,
            gov_nullifier: self.gov_nullifier,
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }

    /// Generate prover witnesses for the circuit, in the order of its `witness` block.
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Base gov_secret
            Witness::Base(Value::known(self.gov_secret)),
            // Base gov_pub_x
            Witness::Base(Value::known(self.gov_pub_x)),
            // Base gov_pub_y
            Witness::Base(Value::known(self.gov_pub_y)),
            // Base config_nullifier
            Witness::Base(Value::known(self.gov_nullifier)),
            // Base tx_commitment
            Witness::Base(Value::known(self.tx_commitment)),
            // Base tx_nonce
            Witness::Base(Value::known(self.tx_nonce)),
            // Base tx_binding
            Witness::Base(Value::known(poseidon_hash([
                pallas::Base::from(3u64),
                self.tx_commitment,
                self.tx_nonce,
            ]))),
        ]
    }
}

/// Create an UpdateConfig ZK proof
pub fn create_update_config_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &UpdateConfigCallData,
) -> Result<(Proof, UpdateConfigPublicInputs)> {
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
