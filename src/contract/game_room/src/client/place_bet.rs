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

//! GameRoom PlaceBet ZK proof generation (PlaceBetV2 circuit).
//!
//! witness (10): room_id, pot_id, player_pub_x, player_pub_y, amount, block_height, nonce,
//!               tx_commitment, tx_nonce, tx_binding.
//! `bet_id = poseidon_hash(4, pot_id, player_pub_x, amount, block_height)`;
//! `commitment = poseidon_hash(4, amount, nonce, block_height)`.
//! instances (4): bet_id, tx_binding, tx_nonce, commitment.

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
pub struct PlaceBetPublicInputs {
    pub bet_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub commitment: pallas::Base,
}

impl PlaceBetPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.bet_id, self.tx_binding, self.tx_nonce, self.commitment]
    }
}

#[derive(Debug, Clone)]
pub struct PlaceBetCallData {
    pub room_id: pallas::Base,
    pub pot_id: pallas::Base,
    pub player_pub_x: pallas::Base,
    pub player_pub_y: pallas::Base,
    pub amount: u64,
    pub block_height: u64,
    pub nonce: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PlaceBetCallData {
    pub fn new(room_id: pallas::Base, pot_id: pallas::Base, player_public: PublicKey, amount: u64, block_height: u64, nonce: pallas::Base) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (px, py) = player_public.xy().expect("pk not identity");
        Self {
            room_id,
            pot_id,
            player_pub_x: px,
            player_pub_y: py,
            amount,
            block_height,
            nonce,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> PlaceBetPublicInputs {
        let bet_id = poseidon_hash([
            pallas::Base::from(4u64),
            self.pot_id,
            self.player_pub_x,
            pallas::Base::from(self.amount),
            pallas::Base::from(self.block_height),
        ]);
        let commitment = poseidon_hash([
            pallas::Base::from(4u64),
            pallas::Base::from(self.amount),
            self.nonce,
            pallas::Base::from(self.block_height),
        ]);
        PlaceBetPublicInputs {
            bet_id,
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
            commitment,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.room_id)),
            Witness::Base(Value::known(self.pot_id)),
            Witness::Base(Value::known(self.player_pub_x)),
            Witness::Base(Value::known(self.player_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.amount))),
            Witness::Base(Value::known(pallas::Base::from(self.block_height))),
            Witness::Base(Value::known(self.nonce)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))),
        ]
    }
}

pub fn place_bet_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PlaceBetCallData,
) -> Result<(Proof, PlaceBetPublicInputs)> {
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
