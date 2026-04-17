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

//! Escrow claim_v1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, pasta_prelude::PrimeField, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// ClaimEscrowV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct ClaimEscrowPublicInputs {
    pub escrow_id: pallas::Base,
    pub escrow_seller_commitment: pallas::Base,
    pub spent_nullifier: pallas::Base,
}

impl ClaimEscrowPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.escrow_id, self.escrow_seller_commitment, self.spent_nullifier]
    }
}

/// Input data for claim_escrow proof generation
#[derive(Debug, Clone)]
pub struct ClaimEscrowCallData {
    pub escrow_id: pallas::Base,
    pub seller_secret: pallas::Base,
    pub seller_pubkey: PublicKey,
    pub escrow_seller_commitment: pallas::Base,
}

impl ClaimEscrowCallData {
    pub fn new(
        escrow_id: pallas::Base,
        seller_secret: pallas::Base,
        seller_pubkey: PublicKey,
        escrow_seller_commitment: pallas::Base,
    ) -> Self {
        Self {
            escrow_id,
            seller_secret,
            seller_pubkey,
            escrow_seller_commitment,
        }
    }

    /// Compute nullifier from escrow_id and seller_secret
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.escrow_id, self.seller_secret])
    }

    pub fn compute_public_inputs(&self) -> ClaimEscrowPublicInputs {
        ClaimEscrowPublicInputs {
            escrow_id: self.escrow_id,
            escrow_seller_commitment: self.escrow_seller_commitment,
            spent_nullifier: self.compute_nullifier(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (sx, sy) = self.seller_pubkey.xy();
        vec![
            // Witnesses (must match circuit order: escrow_id, seller_secret, seller_x, seller_y, escrow_seller_commitment)
            Witness::Base(Value::known(self.escrow_id)),
            Witness::Base(Value::known(self.seller_secret)),
            Witness::Base(Value::known(sx)),
            Witness::Base(Value::known(sy)),
            Witness::Base(Value::known(self.escrow_seller_commitment)),
        ]
    }
}

/// Create a ClaimEscrow ZK proof
pub fn create_claim_escrow_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ClaimEscrowCallData,
) -> Result<(Proof, ClaimEscrowPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}