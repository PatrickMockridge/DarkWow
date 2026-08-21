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

//! GameRoom identity-proof ZK generation (shared by 6 functions).
//!
//! withdraw, raise, call, fold, close_pot, contribute_entropy share the same layout:
//! witness (8): room_id, player_secret, player_pub_x, player_pub_y, player_nullifier,
//!              tx_commitment, tx_nonce, tx_binding.
//! `player_pub = ec_mul_base(player_secret, NULLIFIER_K)` bound to pub_x/y;
//! `player_nullifier = poseidon_hash(1, room_id, player_secret)`.
//! instances (5): player_pub_x, player_pub_y, player_nullifier, tx_binding, tx_nonce.

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
pub struct IdentityPublicInputs {
    pub pub_x: pallas::Base,
    pub pub_y: pallas::Base,
    pub nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl IdentityPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.pub_x, self.pub_y, self.nullifier, self.tx_binding, self.tx_nonce]
    }
}

#[derive(Debug, Clone)]
pub struct IdentityCallData {
    pub room_id: pallas::Base,
    pub player_secret: pallas::Base,
    pub player_pub_x: pallas::Base,
    pub player_pub_y: pallas::Base,
    pub player_nullifier: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl IdentityCallData {
    pub fn new(room_id: pallas::Base, player_public: PublicKey, player_secret: pallas::Base, domain: u64) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (px, py) = player_public.xy().expect("pk not identity");
        let player_nullifier = poseidon_hash([pallas::Base::from(domain), room_id, player_secret]);
        Self {
            room_id,
            player_secret,
            player_pub_x: px,
            player_pub_y: py,
            player_nullifier,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> IdentityPublicInputs {
        IdentityPublicInputs {
            pub_x: self.player_pub_x,
            pub_y: self.player_pub_y,
            nullifier: self.player_nullifier,
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.room_id)),
            Witness::Base(Value::known(self.player_secret)),
            Witness::Base(Value::known(self.player_pub_x)),
            Witness::Base(Value::known(self.player_pub_y)),
            Witness::Base(Value::known(self.player_nullifier)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))),
        ]
    }
}

pub fn create_identity_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    data: &IdentityCallData,
) -> Result<(Proof, IdentityPublicInputs)> {
    let pi = data.compute_public_inputs();
    let c = ZkCircuit::new(data.to_witnesses(), zkbin);
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
