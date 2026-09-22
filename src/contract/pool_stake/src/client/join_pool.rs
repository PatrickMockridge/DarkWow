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

//! Pool Stake JoinPool ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, Blind, PublicKey,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// JoinPoolV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct JoinPoolV1PublicInputs {
    pub pool_id: pallas::Base,
    pub member_pub_x: pallas::Base,
    pub member_pub_y: pallas::Base,
    pub stake_amount: pallas::Base,
    pub asset_id: pallas::Base,
    pub nonce: pallas::Base,
    pub derived_member_id: pallas::Base,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl JoinPoolV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // Circuit order (`join_pool.zk`, the `constrain_instance` sequence):
        //   derived_member_id, value_commit_x, tx_binding, tx_nonce, value_commit_y
        // Note where `value_commit_y` sits — **last**, after the tx pair, not beside its x. This
        // was `[.., value_commit_x, value_commit_y, tx_binding, tx_nonce]`, transposed against both
        // the circuit and the host. See `create_pool.rs` for the full note.
        vec![
            self.derived_member_id,
            self.value_commit_x,
            self.tx_binding,
            self.tx_nonce,
            self.value_commit_y,
        ]
    }
}

/// Input data for JoinPool proof generation
#[derive(Debug, Clone)]
pub struct JoinPoolV1CallData {
    pub pool_id: pallas::Base,
    pub member_pub_x: pallas::Base,
    pub member_pub_y: pallas::Base,
    pub stake_amount: u64,
    pub asset_id: pallas::Base,
    pub nonce: u64,
    pub value_blind: pallas::Scalar,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl JoinPoolV1CallData {
    pub fn new(
        pool_id: pallas::Base,
        member_public: PublicKey,
        stake_amount: u64,
        asset_id: pallas::Base,
        nonce: u64,
        value_blind: pallas::Scalar,
    ) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (mx, my) = member_public.xy().expect("pk not identity");
        Self {
            pool_id,
            member_pub_x: mx,
            member_pub_y: my,
            stake_amount,
            asset_id,
            nonce,
            value_blind,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// The circuit's `value_commit = ec_add(ec_mul_short(stake_amount, VALUE_COMMIT_VALUE),
    /// ec_mul(value_blind, VALUE_COMMIT_RANDOM))`, and its two exposed coordinates.
    ///
    /// `pedersen_commitment_u64` derives its bases with
    /// `hash_to_curve("z.cash:Orchard-cv")` over `b"v"`/`b"r"`; the circuit's
    /// `VALUE_COMMIT_VALUE`/`VALUE_COMMIT_RANDOM` are fixed-base *tables over those same points*,
    /// and each table carries a test asserting exactly that (`value_commit_v.rs:814`,
    /// `value_commit_r.rs:2961`). So the helper is the right one, not an approximation.
    ///
    /// The client previously published `Base::zero()` for both coordinates while the circuit
    /// derives and constrains them, so the proof could not be satisfied at all.
    pub fn compute_value_commit(&self) -> (pallas::Base, pallas::Base) {
        // `ScalarBlind` is a type alias for `Blind<pallas::Scalar>`, so the constructor is
        // `Blind(...)` — native_token's client does the same (`transfer/proof.rs:196`).
        let point = pedersen_commitment_u64(self.stake_amount, Blind(self.value_blind));
        #[expect(clippy::expect_used, reason = "a Pedersen commitment to a u64 with a blind is never the identity point")]
        let coords = point
            .to_affine()
            .coordinates()
            .expect("value commitment is not the identity");
        (*coords.x(), *coords.y())
    }

    /// The circuit's `tx_binding = poseidon_hash(3, tx_commitment, tx_nonce)`. Was a literal zero
    /// written as both the public input and the witness — see `create_pool.rs`.
    pub fn compute_tx_binding(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce])
    }

    pub fn compute_public_inputs(&self) -> JoinPoolV1PublicInputs {
        let derived_member_id = poseidon_hash([
            pallas::Base::from(4),
            self.pool_id,
            self.member_pub_x,
            self.member_pub_y,
            pallas::Base::from(self.stake_amount),
            pallas::Base::from(self.nonce),
        ]);
        let (value_commit_x, value_commit_y) = self.compute_value_commit();
        JoinPoolV1PublicInputs {
            pool_id: self.pool_id,
            member_pub_x: self.member_pub_x,
            member_pub_y: self.member_pub_y,
            stake_amount: pallas::Base::from(self.stake_amount),
            asset_id: self.asset_id,
            nonce: pallas::Base::from(self.nonce),
            derived_member_id,
            value_commit_x,
            value_commit_y,
            tx_binding: self.compute_tx_binding(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.pool_id)),
            Witness::Base(Value::known(self.member_pub_x)),
            Witness::Base(Value::known(self.member_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.stake_amount))),
            Witness::Base(Value::known(self.asset_id)),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            // Private inputs
            Witness::Scalar(Value::known(self.value_blind)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.compute_tx_binding())), // tx_binding
        ]
    }
}

/// Create a JoinPool ZK proof
pub fn join_pool_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &JoinPoolV1CallData,
) -> Result<(Proof, JoinPoolV1PublicInputs)> {
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