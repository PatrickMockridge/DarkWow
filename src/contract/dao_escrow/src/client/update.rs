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

//! DAO-Escrow `UpdateV1` (0x01) ZK proof generation — the governance setter (`OBL-C151`).
//!
//! The circuit is `proof/set_governance_config.zk` (`SetGovernanceConfigV2`), which was already in the
//! tree and already loaded by the harness; it proves **ownership**, deriving
//! `owner_pub = ec_mul_base(owner_secret, NULLIFIER_K)` and constraining the exposed `owner_pub_x/y` to
//! it, and derives an `owner_nullifier` the contract records to make the proof one-shot. This module is
//! the caller it never had.

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

/// `SetGovernanceConfigV2` circuit public inputs, in the circuit's `constrain_instance` order.
#[derive(Debug, Clone)]
pub struct UpdateV1PublicInputs {
    pub owner_pub_x: pallas::Base,
    pub owner_pub_y: pallas::Base,
    pub owner_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UpdateV1PublicInputs {
    /// The five values `set_governance_config.zk:37-41` publishes, in that order. `update_get_metadata`
    /// publishes the same five from the params, so a proof whose vector disagrees is one over a vector
    /// the verifier never asks for — the class the metadata gate names in its own words.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.owner_pub_x,
            self.owner_pub_y,
            self.owner_nullifier,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for the `UpdateV1` proof.
#[derive(Debug, Clone)]
pub struct UpdateV1CallData {
    pub owner_secret: pallas::Base,
    pub owner_pub_x: pallas::Base,
    pub owner_pub_y: pallas::Base,
    pub owner_nullifier: pallas::Base,
    pub dao_escrow_bulla: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UpdateV1CallData {
    /// `owner_public` is taken rather than derived so that a caller which supplies the wrong one gets a
    /// proof that cannot satisfy `constrain_equal_base(ec_get_x(owner_pub), owner_pub_x)` — a failure at
    /// proving time, not a wrong proof.
    pub fn new(
        owner_secret: pallas::Base,
        owner_public: PublicKey,
        dao_escrow_bulla: pallas::Base,
    ) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ox, oy) = owner_public.xy().expect("pk not identity");
        // `owner_nullifier = poseidon_hash(DOMAIN_NULLIFIER = 1, owner_pub_x, owner_pub_y, owner_secret,
        // dao_escrow_bulla)` — the circuit's own derivation, host-side. The `1` is a literal in the
        // circuit (`DOMAIN_NULLIFIER = witness_base(1)`), not a witness index.
        let owner_nullifier =
            poseidon_hash([pallas::Base::from(1u64), ox, oy, owner_secret, dao_escrow_bulla]);
        Self {
            owner_secret,
            owner_pub_x: ox,
            owner_pub_y: oy,
            owner_nullifier,
            dao_escrow_bulla,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_tx_binding(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce])
    }

    pub fn compute_public_inputs(&self) -> UpdateV1PublicInputs {
        UpdateV1PublicInputs {
            owner_pub_x: self.owner_pub_x,
            owner_pub_y: self.owner_pub_y,
            owner_nullifier: self.owner_nullifier,
            tx_binding: self.compute_tx_binding(),
            tx_nonce: self.tx_nonce,
        }
    }

    /// The circuit's witnesses, in the order its `witness` block declares them
    /// (`set_governance_config.zk:5-13`): `owner_secret, owner_pub_x, owner_pub_y, owner_nullifier,
    /// tx_commitment, tx_nonce, tx_binding, dao_escrow_bulla` — **eight**.
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.owner_secret)),
            Witness::Base(Value::known(self.owner_pub_x)),
            Witness::Base(Value::known(self.owner_pub_y)),
            Witness::Base(Value::known(self.owner_nullifier)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.compute_tx_binding())),
            Witness::Base(Value::known(self.dao_escrow_bulla)),
        ]
    }
}

/// Create the `UpdateV1` ownership proof.
pub fn update_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &UpdateV1CallData,
) -> Result<(Proof, UpdateV1PublicInputs)> {
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
