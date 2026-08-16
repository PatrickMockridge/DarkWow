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

//! GameRoom CreateRoom ZK proof generation (CreateRoomV2 circuit).
//!
//! witness (8): owner_pub_x, owner_pub_y, token_id, block_height, nonce,
//!              tx_commitment, tx_nonce, tx_binding.
//! `room_id = poseidon_hash(4, owner_pub_x, owner_pub_y, token_id, block_height, nonce)`.
//! instances (3): tx_binding, tx_nonce, room_id.

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

#[derive(Debug, Clone)]
pub struct CreateRoomPublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub room_id: pallas::Base,
}

impl CreateRoomPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce, self.room_id]
    }
}

#[derive(Debug, Clone)]
pub struct CreateRoomCallData {
    pub owner_pub_x: pallas::Base,
    pub owner_pub_y: pallas::Base,
    pub token_id: pallas::Base,
    pub block_height: u64,
    pub nonce: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CreateRoomCallData {
    pub fn new(owner_public: PublicKey, token_id: pallas::Base, block_height: u64, nonce: pallas::Base) -> Self {
        let (ox, oy) = owner_public.xy().expect("pk not identity");
        Self {
            owner_pub_x: ox,
            owner_pub_y: oy,
            token_id,
            block_height,
            nonce,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> CreateRoomPublicInputs {
        let room_id = poseidon_hash([
            pallas::Base::from(4u64),
            self.owner_pub_x,
            self.owner_pub_y,
            self.token_id,
            pallas::Base::from(self.block_height),
            self.nonce,
        ]);
        CreateRoomPublicInputs {
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
            room_id,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.owner_pub_x)),
            Witness::Base(Value::known(self.owner_pub_y)),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(pallas::Base::from(self.block_height))),
            Witness::Base(Value::known(self.nonce)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))),
        ]
    }
}

pub fn create_room_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateRoomCallData,
) -> Result<(Proof, CreateRoomPublicInputs)> {
    let pi = input.compute_public_inputs();
    let c = ZkCircuit::new(input.to_witnesses(), zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let p = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[c], &pi.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[c], &pi.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let p = Proof::create(pk, &[c], &pi.to_vec(), &mut OsRng)?;
    Ok((p, pi))
}
