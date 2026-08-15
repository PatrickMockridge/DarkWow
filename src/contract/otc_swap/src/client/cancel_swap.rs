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

//! OTC Swap CancelSwapV1 ZK proof generation

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
use rand::SeedableRng;

/// CancelSwapV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CancelSwapPublicInputs {
    pub swap_id: pallas::Base,
    pub timeout: pallas::Base,
    pub current_block: pallas::Base,
    pub alice_x: pallas::Base,
    pub alice_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub spent_nullifier: pallas::Base,
}

impl CancelSwapPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.swap_id,
            self.timeout,
            self.current_block,
            self.alice_x,
            self.alice_y,
            self.tx_binding,
            self.tx_nonce,
            self.spent_nullifier,
        ]
    }
}

/// Input data for cancel_swap proof generation
#[derive(Debug, Clone)]
pub struct CancelSwapCallData {
    pub swap_id: pallas::Base,
    pub alice_secret: pallas::Base,
    pub alice_pubkey: PublicKey,
    pub timeout: u64,
    pub current_block: u64,
    pub recipient_pubkey: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CancelSwapCallData {
    pub fn new(
        swap_id: pallas::Base,
        alice_secret: pallas::Base,
        alice_pubkey: PublicKey,
        timeout: u64,
        current_block: u64,
        recipient_pubkey: PublicKey,
    ) -> Self {
        Self { swap_id, alice_secret, alice_pubkey, timeout, current_block, recipient_pubkey, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    /// Compute spent nullifier: H(swap_id, alice_secret)
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(1), self.swap_id, self.alice_secret])
    }

    pub fn compute_public_inputs(&self) -> CancelSwapPublicInputs {
        let (ax, ay) = self.alice_pubkey.xy().expect("pk not identity");
        CancelSwapPublicInputs {
            swap_id: self.swap_id,
            timeout: pallas::Base::from(self.timeout),
            current_block: pallas::Base::from(self.current_block),
            alice_x: ax,
            alice_y: ay,
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
            spent_nullifier: self.compute_nullifier(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ax, ay) = self.alice_pubkey.xy().expect("pk not identity");
        let (rx, ry) = self.recipient_pubkey.xy().expect("pk not identity");
        vec![
            Witness::Base(Value::known(self.swap_id)),
            Witness::Base(Value::known(pallas::Base::from(self.timeout))),
            Witness::Base(Value::known(pallas::Base::from(self.current_block))),
            Witness::Base(Value::known(ax)),
            Witness::Base(Value::known(ay)),
            Witness::Base(Value::known(self.alice_secret)),
            Witness::Base(Value::known(rx)),
            Witness::Base(Value::known(ry)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))), // tx_binding
        ]
    }
}

/// Create a CancelSwap ZK proof
pub fn cancel_swap_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CancelSwapCallData,
) -> Result<(Proof, CancelSwapPublicInputs)> {
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
