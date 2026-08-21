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

//! GameRoom SettlePot ZK proof generation (SettlePotV2 circuit).
//!
//! witness (10): room_id, pot_id, house_pub_x, house_pub_y, pot_total, num_winners, nonce,
//!               tx_commitment, tx_nonce, tx_binding.
//! `room_id2 = poseidon_hash(4, house_pub_x, house_pub_y, nonce)`;
//! `pot_id2 = poseidon_hash(4, room_id, pot_total, house_pub_x)`.
//! instances (4): room_id2, tx_binding, tx_nonce, pot_id2.

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
pub struct SettlePotPublicInputs {
    pub room_id2: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub pot_id2: pallas::Base,
}

impl SettlePotPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.room_id2, self.tx_binding, self.tx_nonce, self.pot_id2]
    }
}

#[derive(Debug, Clone)]
pub struct SettlePotCallData {
    pub room_id: pallas::Base,
    pub pot_id: pallas::Base,
    pub house_pub_x: pallas::Base,
    pub house_pub_y: pallas::Base,
    pub pot_total: u64,
    pub num_winners: u64,
    pub nonce: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SettlePotCallData {
    pub fn new(room_id: pallas::Base, pot_id: pallas::Base, house_public: PublicKey, pot_total: u64, num_winners: u64, nonce: pallas::Base) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (hx, hy) = house_public.xy().expect("pk not identity");
        Self {
            room_id,
            pot_id,
            house_pub_x: hx,
            house_pub_y: hy,
            pot_total,
            num_winners,
            nonce,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> SettlePotPublicInputs {
        let room_id2 = poseidon_hash([
            pallas::Base::from(4u64),
            self.house_pub_x,
            self.house_pub_y,
            self.nonce,
        ]);
        let pot_id2 = poseidon_hash([
            pallas::Base::from(4u64),
            self.room_id,
            pallas::Base::from(self.pot_total),
            self.house_pub_x,
        ]);
        SettlePotPublicInputs {
            room_id2,
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
            pot_id2,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.room_id)),
            Witness::Base(Value::known(self.pot_id)),
            Witness::Base(Value::known(self.house_pub_x)),
            Witness::Base(Value::known(self.house_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.pot_total))),
            Witness::Base(Value::known(pallas::Base::from(self.num_winners))),
            Witness::Base(Value::known(self.nonce)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))),
        ]
    }
}

pub fn settle_pot_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SettlePotCallData,
) -> Result<(Proof, SettlePotPublicInputs)> {
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
