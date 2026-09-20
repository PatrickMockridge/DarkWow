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

//! ExecuteSwapFee ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// ExecuteSwapFee circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct ExecuteSwapFeePublicInputs {
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
    /// fee = floor(fill_amount * fee_bps / 10000), computed in-circuit
    pub fee: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ExecuteSwapFeePublicInputs {
    /// Convert to vector for ZK proof creation
    /// Order must match constrain_instance calls in execute_swap_fee.zk
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.alice_nullifier,
            self.bob_nullifier,
            self.swap_id,
            self.fee,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for ExecuteSwapFee proof generation
#[derive(Debug, Clone)]
pub struct ExecuteSwapFeeCallData {
    /// Alice's secret for her lock
    pub alice_secret: pallas::Base,
    /// Alice's offered token
    pub alice_token: pallas::Base,
    /// Alice's offered amount
    pub alice_amount: pallas::Base,
    /// Alice's blinding factor for the CapCommitment
    pub alice_blind: pallas::Base,
    /// Alice's lock commitment (public input)
    pub alice_lock: pallas::Base,
    /// Bob's secret for his lock
    pub bob_secret: pallas::Base,
    /// Bob's offered token
    pub bob_token: pallas::Base,
    /// Bob's offered amount
    pub bob_amount: pallas::Base,
    /// Bob's blinding factor for the CapCommitment
    pub bob_blind: pallas::Base,
    /// Bob's lock commitment (public input)
    pub bob_lock: pallas::Base,
    /// Partial fill amount
    pub fill_amount: pallas::Base,
    /// Fee basis points (e.g., 30 = 0.3%)
    pub fee_bps: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ExecuteSwapFeeCallData {
    /// Create new call data
    pub fn new(
        alice_secret: pallas::Base,
        alice_token: pallas::Base,
        alice_amount: pallas::Base,
        alice_lock: pallas::Base,
        bob_secret: pallas::Base,
        bob_token: pallas::Base,
        bob_amount: pallas::Base,
        bob_lock: pallas::Base,
        fill_amount: pallas::Base,
        fee_bps: pallas::Base,
    ) -> Self {
        Self {
            alice_secret,
            alice_token,
            alice_amount,
            alice_blind: poseidon_hash([alice_secret, pallas::Base::from(1u64)]),
            alice_lock,
            bob_secret,
            bob_token,
            bob_amount,
            bob_blind: poseidon_hash([bob_secret, pallas::Base::from(1u64)]),
            bob_lock,
            fill_amount,
            fee_bps,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// `fee = floor(fill_amount * fee_bps / 10000)`, the arithmetic `ExecuteSwapFee_V2` proves.
    ///
    /// `fill_amount` and `fee_bps` arrive as field elements. They are built from `u64`s by every
    /// caller, so they are read back as `u64`s here; if either is not, the fee is set to a value
    /// the circuit's quotient-remainder cannot satisfy, and proving fails rather than the client
    /// silently proving a fee nobody asked for.
    fn fee(&self) -> pallas::Base {
        fn as_u64(x: pallas::Base) -> Option<u64> {
            let repr = x.to_repr();
            if repr[8..].iter().any(|byte| *byte != 0) {
                return None;
            }
            let mut low = [0u8; 8];
            low.copy_from_slice(&repr[..8]);
            Some(u64::from_le_bytes(low))
        }

        match (as_u64(self.fill_amount), as_u64(self.fee_bps)) {
            (Some(fill), Some(bps)) => {
                let quotient = (u128::from(fill) * u128::from(bps)) / 10_000u128;
                match u64::try_from(quotient) {
                    Ok(fee) => pallas::Base::from(fee),
                    Err(_) => pallas::Base::from(u64::MAX),
                }
            }
            _ => pallas::Base::zero(),
        }
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> ExecuteSwapFeePublicInputs {
        // Compute Alice's nullifier
        let alice_nullifier = poseidon_hash([pallas::Base::from(1u64), self.alice_secret, self.alice_lock]);

        // Compute Bob's nullifier
        let bob_nullifier = poseidon_hash([pallas::Base::from(1u64), self.bob_secret, self.bob_lock]);

        // Compute swap ID
        let swap_id = poseidon_hash([pallas::Base::from(4u64), self.alice_lock, self.bob_token, self.bob_amount]);

        ExecuteSwapFeePublicInputs {
            alice_lock: self.alice_lock,
            bob_lock: self.bob_lock,
            alice_nullifier,
            bob_nullifier,
            swap_id,
            fee: self.fee(),
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }

    /// Generate prover witnesses for the circuit
    ///
    /// The fee must be the same value `compute_public_inputs` exposes, so both read it from
    /// `self.fee()`.
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let fee = self.fee();
        vec![
            // Base alice_secret
            Witness::Base(Value::known(self.alice_secret)),
            // Base alice_token
            Witness::Base(Value::known(self.alice_token)),
            // Base alice_amount
            Witness::Base(Value::known(self.alice_amount)),
            // Base alice_blind
            Witness::Base(Value::known(self.alice_blind)),
            // Base alice_lock (public input)
            Witness::Base(Value::known(self.alice_lock)),
            // Base bob_secret
            Witness::Base(Value::known(self.bob_secret)),
            // Base bob_token
            Witness::Base(Value::known(self.bob_token)),
            // Base bob_amount
            Witness::Base(Value::known(self.bob_amount)),
            // Base bob_blind
            Witness::Base(Value::known(self.bob_blind)),
            // Base bob_lock (public input)
            Witness::Base(Value::known(self.bob_lock)),
            // Base fill_amount
            Witness::Base(Value::known(self.fill_amount)),
            // Base fee_bps
            Witness::Base(Value::known(self.fee_bps)),
            // Base fee — witness order must match execute_swap_fee.zk
            Witness::Base(Value::known(fee)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))), // tx_binding
        ]
    }
}

/// Create an ExecuteSwapFee ZK proof
///
/// # Arguments
///
/// * `zkbin` - The compiled ZK binary for ExecuteSwapFee circuit
/// * `pk` - The proving key for the circuit
/// * `input` - The call data containing secrets and parameters
///
/// # Returns
///
/// * `(Proof, ExecuteSwapFeePublicInputs)` - The ZK proof and public inputs
pub fn create_execute_swap_fee_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ExecuteSwapFeeCallData,
) -> Result<(Proof, ExecuteSwapFeePublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    //dwow_core::zk::export_witness_json("proof/witness/execute_swap_fee_v1.json", &witnesses, &public_inputs.to_vec());
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
