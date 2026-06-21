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

//! OTC Swap ExecuteSwapV1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// ExecuteSwapV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct ExecuteSwapPublicInputs {
    pub swap_id: pallas::Base,
    pub bob_commitment: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub spent_nullifier: pallas::Base,
}

impl ExecuteSwapPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.swap_id, self.bob_commitment, self.tx_binding,
            self.tx_nonce, self.spent_nullifier]
    }
}

/// Input data for execute_swap proof generation
#[derive(Debug, Clone)]
pub struct ExecuteSwapCallData {
    pub swap_id: pallas::Base,
    pub bob_secret: pallas::Base,
    pub bob_pubkey: PublicKey,
    pub alice_recipient: PublicKey,
    pub bob_recipient: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ExecuteSwapCallData {
    pub fn new(
        swap_id: pallas::Base,
        bob_secret: pallas::Base,
        bob_pubkey: PublicKey,
        alice_recipient: PublicKey,
        bob_recipient: PublicKey,
    ) -> Self {
        Self { swap_id, bob_secret, bob_pubkey, alice_recipient, bob_recipient, tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero() }
    }

    /// Compute Bob commitment: H(bob_pub.x, bob_pub.y)
    pub fn compute_bob_commitment(&self) -> pallas::Base {
        let (bx, by) = self.bob_pubkey.xy();
        poseidon_hash([bx, by])
    }

    /// Compute spent nullifier: H(swap_id, bob_secret)
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.swap_id, self.bob_secret])
    }

    pub fn compute_public_inputs(&self) -> ExecuteSwapPublicInputs {
        ExecuteSwapPublicInputs {
            swap_id: self.swap_id,
            bob_commitment: self.compute_bob_commitment(),
            tx_binding: poseidon_hash([self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
            spent_nullifier: self.compute_nullifier(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (bx, by) = self.bob_pubkey.xy();
        let (arx, ary) = self.alice_recipient.xy();
        let (brx, bry) = self.bob_recipient.xy();
        vec![
            Witness::Base(Value::known(self.swap_id)),
            Witness::Base(Value::known(self.bob_secret)),
            Witness::Base(Value::known(bx)),
            Witness::Base(Value::known(by)),
            Witness::Base(Value::known(arx)),
            Witness::Base(Value::known(ary)),
            Witness::Base(Value::known(brx)),
            Witness::Base(Value::known(bry)),
        ]
    }
}

/// Create an ExecuteSwap ZK proof
pub fn execute_swap_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ExecuteSwapCallData,
) -> Result<(Proof, ExecuteSwapPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
