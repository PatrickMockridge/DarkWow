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

//! DAO-Escrow Init ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey, pasta_prelude::PrimeField},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// InitV2 circuit public inputs (4 — matching InitV2 circuit constrain_instance order)
/// Circuit order: [dao_bulla, tx_binding, tx_nonce, endowment_bulla]
#[derive(Debug, Clone)]
pub struct InitV1PublicInputs {
    pub dao_bulla: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub endowment_bulla: pallas::Base,
}

impl InitV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.dao_bulla,
            self.tx_binding,
            self.tx_nonce,
            self.endowment_bulla,
        ]
    }
}

/// Input data for Init proof generation
#[derive(Debug, Clone)]
pub struct InitV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub dao_bulla: pallas::Base,
    pub owner_secret: pallas::Base,
    pub owner_pub_x: pallas::Base,
    pub owner_pub_y: pallas::Base,
    pub endowment_asset_id: pallas::Base,
    pub bulla_blind: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl InitV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        dao_bulla: pallas::Base,
        owner_secret: pallas::Base,
        endowment_asset_id: pallas::Base,
        bulla_blind: pallas::Base,
    ) -> Self {
        // Derive owner public key from secret
        let owner_pub = PublicKey::from_secret(
            dwow_sdk::crypto::SecretKey::from_bytes(owner_secret.to_repr()).unwrap()
        );
        let (ox, oy) = owner_pub.xy().expect("pk not identity");
        Self {
            nullifier_k,
            dao_bulla,
            owner_secret,
            owner_pub_x: ox,
            owner_pub_y: oy,
            endowment_asset_id,
            bulla_blind,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> InitV1PublicInputs {
        // endowment_bulla = poseidon_hash(DOMAIN_COIN_COMMIT, dao_bulla, owner_pub_x, owner_pub_y,
        //                                  endowment_asset_id, bulla_blind)
        let endowment_bulla = poseidon_hash([
            pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
            self.dao_bulla,
            self.owner_pub_x,
            self.owner_pub_y,
            self.endowment_asset_id,
            self.bulla_blind,
        ]);
        InitV1PublicInputs {
            dao_bulla: self.dao_bulla,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
            endowment_bulla,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // nullifier_k is a CONSTANT in circuit - do NOT pass as witness
            // owner_pub_x/y are DERIVED inside circuit from owner_secret and NULLIFIER_K
            Witness::Base(Value::known(self.dao_bulla)),
            Witness::Base(Value::known(self.owner_secret)),
            Witness::Base(Value::known(self.endowment_asset_id)),
            Witness::Base(Value::known(self.bulla_blind)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create an Init ZK proof
pub fn init_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &InitV1CallData,
) -> Result<(Proof, InitV1PublicInputs)> {
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