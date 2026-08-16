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

//! DarkBet Exchange auth/nullifier ZK proof generation (shared by 6 functions).
//!
//! cancel_order, match_orders, place_back, place_lay, remove_liquidity, resolve_market all share the
//! same witness/instance layout:
//! witness (8): id, secret, pub_x, pub_y, nullifier, tx_commitment, tx_nonce, tx_binding.
//! `pub = ec_mul_base(secret, NULLIFIER_K)` bound to pub_x/y; `nullifier = poseidon_hash(1, id, secret)`.
//! instances (5): pub_x, pub_y, nullifier, tx_binding, tx_nonce.

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

#[derive(Debug, Clone)]
pub struct AuthPublicInputs {
    pub pub_x: pallas::Base,
    pub pub_y: pallas::Base,
    pub nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AuthPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.pub_x, self.pub_y, self.nullifier, self.tx_binding, self.tx_nonce]
    }
}

#[derive(Debug, Clone)]
pub struct AuthCallData {
    pub id: pallas::Base,
    pub secret: pallas::Base,
    pub pub_x: pallas::Base,
    pub pub_y: pallas::Base,
    pub nullifier: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AuthCallData {
    pub fn new(id: pallas::Base, secret: pallas::Base) -> Self {
        let pub_ = PublicKey::from_secret(SecretKey::from_base(secret));
        let (px, py) = pub_.xy().expect("pk not identity");
        let nullifier = poseidon_hash([pallas::Base::from(1u64), id, secret]);
        Self {
            id,
            secret,
            pub_x: px,
            pub_y: py,
            nullifier,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> AuthPublicInputs {
        AuthPublicInputs {
            pub_x: self.pub_x,
            pub_y: self.pub_y,
            nullifier: self.nullifier,
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }
}

pub fn create_auth_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    data: &AuthCallData,
) -> Result<(Proof, AuthPublicInputs)> {
    let pi = data.compute_public_inputs();
    let w = vec![
        Witness::Base(Value::known(data.id)),
        Witness::Base(Value::known(data.secret)),
        Witness::Base(Value::known(data.pub_x)),
        Witness::Base(Value::known(data.pub_y)),
        Witness::Base(Value::known(data.nullifier)),
        Witness::Base(Value::known(data.tx_commitment)),
        Witness::Base(Value::known(data.tx_nonce)),
        Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), data.tx_commitment, data.tx_nonce]))), // tx_binding
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
