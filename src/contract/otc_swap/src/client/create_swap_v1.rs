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

//! OTC Swap CreateSwapV1 ZK proof generation

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

/// CreateSwapV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreateSwapPublicInputs {
    pub commitment: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub bob_commitment: pallas::Base,
}

impl CreateSwapPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.commitment, self.tx_commitment, self.bob_commitment]
    }
}

/// Input data for create_swap proof generation
#[derive(Debug, Clone)]
pub struct CreateSwapCallData {
    pub alice_secret: pallas::Base,
    pub alice_pubkey: PublicKey,
    pub bob_pubkey: PublicKey,
    pub send_value: u64,
    pub send_token_id: pallas::Base,
    pub recv_value: u64,
    pub recv_token_id: pallas::Base,
    pub timeout: u64,
    pub tx_commitment: pallas::Base,
}

impl CreateSwapCallData {
    pub fn new(
        alice_secret: pallas::Base,
        alice_pubkey: PublicKey,
        bob_pubkey: PublicKey,
        send_value: u64,
        send_token_id: pallas::Base,
        recv_value: u64,
        recv_token_id: pallas::Base,
        timeout: u64,
    ) -> Self {
        Self {
            alice_secret,
            alice_pubkey,
            bob_pubkey,
            send_value,
            send_token_id,
            recv_value,
            recv_token_id,
            timeout,
            tx_commitment: pallas::Base::zero(),
        }
    }

    /// Compute Bob commitment: H(bob_pub.x, bob_pub.y)
    pub fn compute_bob_commitment(&self) -> pallas::Base {
        let (bx, by) = self.bob_pubkey.xy();
        poseidon_hash([bx, by])
    }

    /// Compute swap commitment
    pub fn compute_commitment(&self) -> pallas::Base {
        let (ax, ay) = self.alice_pubkey.xy();
        let bob_commit = self.compute_bob_commitment();
        poseidon_hash([
            ax,
            ay,
            bob_commit,
            pallas::Base::from(self.send_value),
            self.send_token_id,
            pallas::Base::from(self.recv_value),
            self.recv_token_id,
            pallas::Base::from(self.timeout),
        ])
    }

    pub fn compute_public_inputs(&self) -> CreateSwapPublicInputs {
        CreateSwapPublicInputs {
            commitment: self.compute_commitment(),
            tx_commitment: self.tx_commitment,
            bob_commitment: self.compute_bob_commitment(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ax, ay) = self.alice_pubkey.xy();
        let (bx, by) = self.bob_pubkey.xy();
        vec![
            Witness::Base(Value::known(ax)),
            Witness::Base(Value::known(ay)),
            Witness::Base(Value::known(bx)),
            Witness::Base(Value::known(by)),
            Witness::Base(Value::known(pallas::Base::from(self.send_value))),
            Witness::Base(Value::known(self.send_token_id)),
            Witness::Base(Value::known(pallas::Base::from(self.recv_value))),
            Witness::Base(Value::known(self.recv_token_id)),
            Witness::Base(Value::known(pallas::Base::from(self.timeout))),
            Witness::Base(Value::known(self.alice_secret)),
        ]
    }
}

/// Create a CreateSwap ZK proof
pub fn create_swap_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateSwapCallData,
) -> Result<(Proof, CreateSwapPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
