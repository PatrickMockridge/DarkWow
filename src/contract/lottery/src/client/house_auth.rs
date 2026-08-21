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

//! Lottery house-auth ZK proof generation (DrawWinnersV2 / ExpireLotteryV2 / InitializeV2).
//!
//! All three house-auth circuits share the same witness/instance layout:
//! witness (8): lottery_id, house_secret, house_pub_x, house_pub_y, house_nullifier,
//!   tx_commitment, tx_nonce, tx_binding.
//! `house_pub = ec_mul_base(house_secret, NULLIFIER_K)` bound to house_pub_x/y;
//! `house_nullifier = poseidon_hash(1, lottery_id, house_secret)`.
//! instances (5): house_pub_x, house_pub_y, house_nullifier, tx_binding, tx_nonce.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// House-auth circuit public inputs
#[derive(Debug, Clone)]
pub struct HouseAuthPublicInputs {
    pub house_pub_x: pallas::Base,
    pub house_pub_y: pallas::Base,
    pub house_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl HouseAuthPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.house_pub_x,
            self.house_pub_y,
            self.house_nullifier,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for a house-auth proof (draw_winners / expire_lottery / initialize).
#[derive(Debug, Clone)]
pub struct HouseAuthCallData {
    pub lottery_id: pallas::Base,
    pub house_secret: pallas::Base,
    pub house_pub_x: pallas::Base,
    pub house_pub_y: pallas::Base,
    pub house_nullifier: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl HouseAuthCallData {
    /// Derive house_pub + house_nullifier from the secret + lottery_id.
    pub fn new(lottery_id: pallas::Base, house_secret: pallas::Base) -> Self {
        let house_pub = PublicKey::from_secret(SecretKey::from_base(house_secret));
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (hx, hy) = house_pub.xy().expect("pk not identity");
        let house_nullifier = poseidon_hash([pallas::Base::from(1u64), lottery_id, house_secret]);
        Self {
            lottery_id,
            house_secret,
            house_pub_x: hx,
            house_pub_y: hy,
            house_nullifier,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> HouseAuthPublicInputs {
        HouseAuthPublicInputs {
            house_pub_x: self.house_pub_x,
            house_pub_y: self.house_pub_y,
            house_nullifier: self.house_nullifier,
            tx_binding: poseidon_hash([
                pallas::Base::from(3u64),
                self.tx_commitment,
                self.tx_nonce,
            ]),
            tx_nonce: self.tx_nonce,
        }
    }
}

/// Create a house-auth ZK proof (for the given circuit's zkbin + proving key).
pub fn create_house_auth_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    data: &HouseAuthCallData,
) -> Result<(Proof, HouseAuthPublicInputs)> {
    let pi = data.compute_public_inputs();
    let w = vec![
        Witness::Base(Value::known(data.lottery_id)),
        Witness::Base(Value::known(data.house_secret)),
        Witness::Base(Value::known(data.house_pub_x)),
        Witness::Base(Value::known(data.house_pub_y)),
        Witness::Base(Value::known(data.house_nullifier)),
        Witness::Base(Value::known(data.tx_commitment)),
        Witness::Base(Value::known(data.tx_nonce)),
        Witness::Base(Value::known(poseidon_hash([
            pallas::Base::from(3u64),
            data.tx_commitment,
            data.tx_nonce,
        ]))), // tx_binding
    ];
    let c = ZkCircuit::new(w, zkbin);
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
