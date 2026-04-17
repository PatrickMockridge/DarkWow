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

//! Escrow create_escrow ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, pasta_prelude::PrimeField, PublicKey, SecretKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// CreateEscrowV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreateEscrowPublicInputs {
    pub commitment: pallas::Base,
    pub seller_commitment: pallas::Base,
}

impl CreateEscrowPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.commitment, self.seller_commitment]
    }
}

/// Input data for create_escrow proof generation
#[derive(Debug, Clone)]
pub struct CreateEscrowCallData {
    pub buyer_secret: pallas::Base,
    pub buyer_pubkey: PublicKey,
    pub seller_pubkey: PublicKey,
    pub value: u64,
    pub token_id: pallas::Base,
    pub timeout: u64,
}

impl CreateEscrowCallData {
    pub fn new(
        buyer_secret: pallas::Base,
        buyer_pubkey: PublicKey,
        seller_pubkey: PublicKey,
        value: u64,
        token_id: pallas::Base,
        timeout: u64,
    ) -> Self {
        Self {
            buyer_secret,
            buyer_pubkey,
            seller_pubkey,
            value,
            token_id,
            timeout,
        }
    }

    /// Compute seller commitment: H(seller_pub.x, seller_pub.y)
    pub fn compute_seller_commitment(&self) -> pallas::Base {
        let (sx, sy) = self.seller_pubkey.xy();
        poseidon_hash([sx, sy])
    }

    /// Compute escrow commitment
    pub fn compute_commitment(&self) -> pallas::Base {
        let (bx, by) = self.buyer_pubkey.xy();
        let seller_commit = self.compute_seller_commitment();
        poseidon_hash([
            bx,
            by,
            seller_commit,
            pallas::Base::from(self.value),
            self.token_id,
            pallas::Base::from(self.timeout),
        ])
    }

    pub fn compute_public_inputs(&self) -> CreateEscrowPublicInputs {
        CreateEscrowPublicInputs {
            commitment: self.compute_commitment(),
            seller_commitment: self.compute_seller_commitment(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (bx, by) = self.buyer_pubkey.xy();
        let (sx, sy) = self.seller_pubkey.xy();
        vec![
            // Witnesses (must match circuit order: buyer_pub_x, buyer_pub_y, seller_pub_x, seller_pub_y, value, token_id, timeout, buyer_secret)
            Witness::Base(Value::known(bx)),
            Witness::Base(Value::known(by)),
            Witness::Base(Value::known(sx)),
            Witness::Base(Value::known(sy)),
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(pallas::Base::from(self.timeout))),
            Witness::Base(Value::known(self.buyer_secret)),
        ]
    }
}

/// Create a CreateEscrow ZK proof
pub fn create_escrow_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateEscrowCallData,
) -> Result<(Proof, CreateEscrowPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}