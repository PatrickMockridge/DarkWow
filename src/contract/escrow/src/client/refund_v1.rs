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

//! Escrow refund_v1 ZK proof generation

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

/// RefundEscrowV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct RefundEscrowPublicInputs {
    pub escrow_id: pallas::Base,
    pub timeout: pallas::Base,
    pub current_block: pallas::Base,
    pub input_buyer_pub_x: pallas::Base,
    pub input_buyer_pub_y: pallas::Base,
    pub spent_nullifier: pallas::Base,
}

impl RefundEscrowPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.escrow_id,
            self.timeout,
            self.current_block,
            self.input_buyer_pub_x,
            self.input_buyer_pub_y,
            self.spent_nullifier,
        ]
    }
}

/// Input data for refund_escrow proof generation
#[derive(Debug, Clone)]
pub struct RefundEscrowCallData {
    pub escrow_id: pallas::Base,
    pub timeout: u64,
    pub current_block: u64,
    pub buyer_secret: pallas::Base,
    pub buyer_pubkey: PublicKey,
    pub escrow_buyer_pub_x: pallas::Base,
    pub escrow_buyer_pub_y: pallas::Base,
}

impl RefundEscrowCallData {
    pub fn new(
        escrow_id: pallas::Base,
        timeout: u64,
        current_block: u64,
        buyer_secret: pallas::Base,
        buyer_pubkey: PublicKey,
        escrow_buyer_pub_x: pallas::Base,
        escrow_buyer_pub_y: pallas::Base,
    ) -> Self {
        Self {
            escrow_id,
            timeout,
            current_block,
            buyer_secret,
            buyer_pubkey,
            escrow_buyer_pub_x,
            escrow_buyer_pub_y,
        }
    }

    /// Compute nullifier from escrow_id and buyer_secret
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.escrow_id, self.buyer_secret])
    }

    pub fn compute_public_inputs(&self) -> RefundEscrowPublicInputs {
        let (bx, by) = self.buyer_pubkey.xy();
        RefundEscrowPublicInputs {
            escrow_id: self.escrow_id,
            timeout: pallas::Base::from(self.timeout),
            current_block: pallas::Base::from(self.current_block),
            input_buyer_pub_x: bx,
            input_buyer_pub_y: by,
            spent_nullifier: self.compute_nullifier(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (bx, by) = self.buyer_pubkey.xy();
        vec![
            // Witnesses (must match circuit order: escrow_id, timeout, current_block, buyer_secret, input_buyer_pub_x, input_buyer_pub_y, escrow_buyer_pub_x, escrow_buyer_pub_y)
            Witness::Base(Value::known(self.escrow_id)),
            Witness::Base(Value::known(pallas::Base::from(self.timeout))),
            Witness::Base(Value::known(pallas::Base::from(self.current_block))),
            Witness::Base(Value::known(self.buyer_secret)),
            Witness::Base(Value::known(bx)),
            Witness::Base(Value::known(by)),
            Witness::Base(Value::known(self.escrow_buyer_pub_x)),
            Witness::Base(Value::known(self.escrow_buyer_pub_y)),
        ]
    }
}

/// Create a RefundEscrow ZK proof
pub fn create_refund_escrow_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RefundEscrowCallData,
) -> Result<(Proof, RefundEscrowPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}