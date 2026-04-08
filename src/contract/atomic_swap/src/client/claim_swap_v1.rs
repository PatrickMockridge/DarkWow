/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! AtomicSwap claim ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, pasta_prelude::PrimeField},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// ClaimSwapV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct ClaimSwapPublicInputs {
    pub nullifier: pallas::Base,
}

impl ClaimSwapPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.nullifier]
    }
}

/// Input data for claim_swap proof generation
#[derive(Debug, Clone)]
pub struct ClaimSwapCallData {
    pub swap_id: pallas::Base,
    pub secret: pallas::Base,
    pub hash: pallas::Base,
    pub timelock: u64,
    pub side: u8,
}

impl ClaimSwapCallData {
    pub fn new(
        swap_id: pallas::Base,
        secret: pallas::Base,
        hash: pallas::Base,
        timelock: u64,
        side: u8,
    ) -> Self {
        Self {
            swap_id,
            secret,
            hash,
            timelock,
            side,
        }
    }

    /// Compute nullifier from swap_id and secret
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.swap_id, self.secret])
    }

    pub fn compute_public_inputs(&self) -> ClaimSwapPublicInputs {
        ClaimSwapPublicInputs {
            nullifier: self.compute_nullifier(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Private inputs (witnesses)
            Witness::Base(Value::known(self.swap_id)),
            Witness::Base(Value::known(self.secret)),
            Witness::Base(Value::known(self.hash)),
            Witness::Uint64(Value::known(self.timelock)),
            Witness::Uint8(Value::known(self.side)),
        ]
    }
}

/// Create a ClaimSwap ZK proof
pub fn create_claim_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ClaimSwapCallData,
) -> Result<(Proof, ClaimSwapPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}