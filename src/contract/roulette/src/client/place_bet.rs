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

//! Roulette place_bet_v1 ZK proof generation

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

/// PlaceBetV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct PlaceBetV1PublicInputs {
    pub bet_id: pallas::Base,
    pub nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PlaceBetV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // Circuit instances: tx_binding, tx_nonce (bet_id/nullifier are constrain_equal_base)
        vec![self.tx_binding, self.tx_nonce]
    }
}

/// Input data for place_bet proof generation
#[derive(Debug, Clone)]
pub struct PlaceBetV1CallData {
    pub table_id: pallas::Base,
    pub player_pub_x: pallas::Base,
    pub player_pub_y: pallas::Base,
    pub bet_type: pallas::Base,
    pub amount: u64,
    pub nonce: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PlaceBetV1CallData {
    pub fn new(
        table_id: pallas::Base,
        player_pub: PublicKey,
        bet_type: u8,
        amount: u64,
        nonce: pallas::Base,
    ) -> Self {
        let (px, py) = player_pub.xy().expect("pk not identity");
        Self {
            table_id,
            player_pub_x: px,
            player_pub_y: py,
            bet_type: pallas::Base::from(bet_type as u64),
            amount,
            nonce,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> PlaceBetV1PublicInputs {
        let bet_id = poseidon_hash([
            pallas::Base::from(4),
            self.table_id,
            self.player_pub_x,
            self.player_pub_y,
            pallas::Base::from(self.amount),
        ]);
        let nullifier = poseidon_hash([pallas::Base::from(4), bet_id, self.nonce]);
        PlaceBetV1PublicInputs { bet_id, nullifier, tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]), tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Witnesses (must match circuit order)
            Witness::Base(Value::known(self.table_id)),
            Witness::Base(Value::known(self.player_pub_x)),
            Witness::Base(Value::known(self.player_pub_y)),
            Witness::Base(Value::known(self.bet_type)),
            Witness::Base(Value::known(pallas::Base::from(self.amount))),
            Witness::Base(Value::known(self.nonce)),
            Witness::Base(Value::known(self.compute_public_inputs().bet_id)),
            Witness::Base(Value::known(self.compute_public_inputs().nullifier)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))), // tx_binding
        ]
    }
}

/// Create a PlaceBet ZK proof
pub fn create_place_bet_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PlaceBetV1CallData,
) -> Result<(Proof, PlaceBetV1PublicInputs)> {
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