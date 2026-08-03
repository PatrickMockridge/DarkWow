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

//! CancelSwap ZK proof generation

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

/// CancelSwap circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct CancelSwapPublicInputs {
    /// Nullifier = poseidon_hash([secret, lock_commitment])
    pub nullifier: pallas::Base,
    /// Swap ID = poseidon_hash([lock_commitment])
    pub swap_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CancelSwapPublicInputs {
    /// Convert to vector for ZK proof creation
    /// Order must match constrain_instance calls in cancel_swap_v1.zk
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.nullifier, self.swap_id, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for CancelSwap proof generation
#[derive(Debug, Clone)]
pub struct CancelSwapCallData {
    /// Swap ID being cancelled
    pub swap_id: pallas::Base,
    /// The lock commitment
    pub lock_commitment: pallas::Base,
    /// The secret to the lock
    pub secret: pallas::Base,
    /// Token being cancelled
    pub token: pallas::Base,
    /// Amount being cancelled
    pub amount: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CancelSwapCallData {
    /// Create new call data
    pub fn new(
        swap_id: pallas::Base,
        lock_commitment: pallas::Base,
        secret: pallas::Base,
        token: pallas::Base,
        amount: u64,
    ) -> Self {
        Self {
            swap_id,
            lock_commitment,
            secret,
            token,
            amount: pallas::Base::from(amount),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> CancelSwapPublicInputs {
        // Compute nullifier
        // nullifier = poseidon_hash([secret, lock_commitment])
        let nullifier = poseidon_hash([self.secret, self.lock_commitment]);

        // Compute swap ID
        // swap_id = poseidon_hash([lock_commitment])
        let computed_swap_id = poseidon_hash([self.lock_commitment]);

        CancelSwapPublicInputs {
            nullifier,
            swap_id: computed_swap_id,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Base swap_id
            Witness::Base(Value::known(self.swap_id)),
            // Base lock_commitment
            Witness::Base(Value::known(self.lock_commitment)),
            // Base secret
            Witness::Base(Value::known(self.secret)),
            // Base token
            Witness::Base(Value::known(self.token)),
            // Base amount
            Witness::Base(Value::known(self.amount)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a CancelSwap ZK proof
///
/// # Arguments
///
/// * `zkbin` - The compiled ZK binary for CancelSwap circuit
/// * `pk` - The proving key for the circuit
/// * `input` - The call data containing secrets and parameters
///
/// # Returns
///
/// * `(Proof, CancelSwapPublicInputs)` - The ZK proof and public inputs
pub fn create_cancel_swap_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CancelSwapCallData,
) -> Result<(Proof, CancelSwapPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    //dwow_core::zk::export_witness_json("proof/witness/cancel_swap_v1.json", &witnesses, &public_inputs.to_vec());
    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
