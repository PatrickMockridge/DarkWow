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

//! ExecuteSwap ZK proof generation

use dwow::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::poseidon_hash,
    pasta::pallas,
};
use rand::rngs::OsRng;

/// ExecuteSwap circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct ExecuteSwapPublicInputs {
    /// Alice's lock commitment
    pub alice_lock: pallas::Base,
    /// Bob's lock commitment
    pub bob_lock: pallas::Base,
    /// Alice's nullifier = poseidon_hash([alice_secret, alice_lock])
    pub alice_nullifier: pallas::Base,
    /// Bob's nullifier = poseidon_hash([bob_secret, bob_lock])
    pub bob_nullifier: pallas::Base,
    /// Swap ID = poseidon_hash([alice_lock, bob_token, bob_amount])
    pub swap_id: pallas::Base,
    /// FuncRef for Alice's OtcSwapV1 child call
    pub alice_otc_func_id: pallas::Base,
    /// FuncRef for Bob's OtcSwapV1 child call
    pub bob_otc_func_id: pallas::Base,
}

impl ExecuteSwapPublicInputs {
    /// Convert to vector for ZK proof creation
    /// Order must match constrain_instance calls in execute_swap_v1.zk
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.alice_nullifier,
            self.bob_nullifier,
            self.swap_id,
            self.alice_otc_func_id,
            self.bob_otc_func_id,
        ]
    }
}

/// Input data for ExecuteSwap proof generation
#[derive(Debug, Clone)]
pub struct ExecuteSwapCallData {
    /// Alice's secret for her lock
    pub alice_secret: pallas::Base,
    /// Alice's offered token
    pub alice_token: pallas::Base,
    /// Alice's offered amount
    pub alice_amount: pallas::Base,
    /// Alice's lock commitment (public input)
    pub alice_lock: pallas::Base,
    /// Bob's secret for his lock
    pub bob_secret: pallas::Base,
    /// Bob's offered token
    pub bob_token: pallas::Base,
    /// Bob's offered amount
    pub bob_amount: pallas::Base,
    /// Bob's lock commitment (public input)
    pub bob_lock: pallas::Base,
    /// Partial fill amount
    pub fill_amount: pallas::Base,
    /// FuncRef for Alice's OtcSwapV1 child call
    pub alice_otc_func_id: pallas::Base,
    /// FuncRef for Bob's OtcSwapV1 child call
    pub bob_otc_func_id: pallas::Base,
}

impl ExecuteSwapCallData {
    /// Create new call data
    pub fn new(
        alice_secret: pallas::Base,
        alice_token: pallas::Base,
        alice_amount: u64,
        alice_lock: pallas::Base,
        bob_secret: pallas::Base,
        bob_token: pallas::Base,
        bob_amount: u64,
        bob_lock: pallas::Base,
        fill_amount: u64,
        alice_otc_func_id: pallas::Base,
        bob_otc_func_id: pallas::Base,
    ) -> Self {
        Self {
            alice_secret,
            alice_token,
            alice_amount: pallas::Base::from(alice_amount),
            alice_lock,
            bob_secret,
            bob_token,
            bob_amount: pallas::Base::from(bob_amount),
            bob_lock,
            fill_amount: pallas::Base::from(fill_amount),
            alice_otc_func_id,
            bob_otc_func_id,
        }
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> ExecuteSwapPublicInputs {
        // Compute Alice's nullifier
        // alice_nullifier = poseidon_hash([alice_secret, alice_lock])
        let alice_nullifier = poseidon_hash([self.alice_secret, self.alice_lock]);

        // Compute Bob's nullifier
        // bob_nullifier = poseidon_hash([bob_secret, bob_lock])
        let bob_nullifier = poseidon_hash([self.bob_secret, self.bob_lock]);

        // Compute swap ID
        // swap_id = poseidon_hash([alice_lock, bob_token, bob_amount])
        let swap_id = poseidon_hash([self.alice_lock, self.bob_token, self.bob_amount]);

        ExecuteSwapPublicInputs {
            alice_lock: self.alice_lock,
            bob_lock: self.bob_lock,
            alice_nullifier,
            bob_nullifier,
            swap_id,
            alice_otc_func_id: self.alice_otc_func_id,
            bob_otc_func_id: self.bob_otc_func_id,
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Base alice_secret
            Witness::Base(Value::known(self.alice_secret)),
            // Base alice_token
            Witness::Base(Value::known(self.alice_token)),
            // Base alice_amount
            Witness::Base(Value::known(self.alice_amount)),
            // Base alice_lock (public input)
            Witness::Base(Value::known(self.alice_lock)),
            // Base bob_secret
            Witness::Base(Value::known(self.bob_secret)),
            // Base bob_token
            Witness::Base(Value::known(self.bob_token)),
            // Base bob_amount
            Witness::Base(Value::known(self.bob_amount)),
            // Base bob_lock (public input)
            Witness::Base(Value::known(self.bob_lock)),
            // Base fill_amount
            Witness::Base(Value::known(self.fill_amount)),
            // Base alice_otc_func_id
            Witness::Base(Value::known(self.alice_otc_func_id)),
            // Base bob_otc_func_id
            Witness::Base(Value::known(self.bob_otc_func_id)),
        ]
    }
}

/// Create an ExecuteSwap ZK proof
///
/// # Arguments
///
/// * `zkbin` - The compiled ZK binary for ExecuteSwap circuit
/// * `pk` - The proving key for the circuit
/// * `input` - The call data containing secrets and parameters
///
/// # Returns
///
/// * `(Proof, ExecuteSwapPublicInputs)` - The ZK proof and public inputs
pub fn create_execute_swap_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ExecuteSwapCallData,
) -> Result<(Proof, ExecuteSwapPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    //dwow::zk::export_witness_json("proof/witness/execute_swap_v1.json", &witnesses, &public_inputs.to_vec());
    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
