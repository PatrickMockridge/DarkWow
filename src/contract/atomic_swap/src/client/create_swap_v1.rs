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

//! AtomicSwap create_swap ZK proof generation

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
    pub swap_id: pallas::Base,
}

impl CreateSwapPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.swap_id]
    }
}

/// Input data for create_swap proof generation
#[derive(Debug, Clone)]
pub struct CreateSwapCallData {
    pub hash: pallas::Base,
    pub timelock: u64,
    pub secret: pallas::Base,
    pub amount: u64,
    pub token_id: pallas::Base,
    pub side: u8,
    pub blind: pallas::Base,
    pub receiver_public: PublicKey,
}

impl CreateSwapCallData {
    pub fn new(
        hash: pallas::Base,
        timelock: u64,
        secret: pallas::Base,
        amount: u64,
        token_id: pallas::Base,
        side: u8,
        blind: pallas::Base,
        receiver_public: PublicKey,
    ) -> Self {
        Self {
            hash,
            timelock,
            secret,
            amount,
            token_id,
            side,
            blind,
            receiver_public,
        }
    }

    /// Compute swap ID from parameters
    pub fn compute_swap_id(&self) -> pallas::Base {
        let (pub_x, pub_y) = self.receiver_public.xy();
        poseidon_hash([
            self.hash,
            pallas::Base::from(self.timelock),
            pub_x,
            pub_y,
            pallas::Base::from(self.amount),
            self.token_id,
            pallas::Base::from(self.side as u64),
            self.blind,
        ])
    }

    pub fn compute_public_inputs(&self) -> CreateSwapPublicInputs {
        CreateSwapPublicInputs {
            swap_id: self.compute_swap_id(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (pub_x, pub_y) = self.receiver_public.xy();
        vec![
            // Private inputs (witnesses) - must match circuit order
            Witness::Base(Value::known(self.compute_swap_id())), // swap_id
            Witness::Base(Value::known(self.secret)),
            Witness::Base(Value::known(self.hash)),
            Witness::Base(Value::known(pallas::Base::from(self.timelock))),
            Witness::Base(Value::known(pallas::Base::from(self.amount))),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(pallas::Base::from(u64::from(self.side)))),
            Witness::Base(Value::known(self.blind)),
            Witness::Base(Value::known(pub_x)),
            Witness::Base(Value::known(pub_y)),
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