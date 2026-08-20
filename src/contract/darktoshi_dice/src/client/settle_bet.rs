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

//! DarkToshi Dice settle_bet_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{crypto::poseidon_hash, pasta::pallas};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// SettleBetV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SettleBetV1PublicInputs {
    pub derived_bet_id: pallas::Base,
    pub roll_hash: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SettleBetV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.derived_bet_id, self.tx_binding, self.tx_nonce, self.roll_hash]
    }
}

/// Input data for settle_bet proof generation
#[derive(Debug, Clone)]
pub struct SettleBetV1CallData {
    pub player_pub_x: pallas::Base,
    pub player_pub_y: pallas::Base,
    pub bet_value: pallas::Base,
    pub target: pallas::Base,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
    pub asset_id: pallas::Base,
    pub block_hash: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SettleBetV1CallData {
    pub fn new(
        player_pub_x: pallas::Base,
        player_pub_y: pallas::Base,
        bet_value: pallas::Base,
        target: pallas::Base,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        asset_id: pallas::Base,
        block_hash: pallas::Base,
    ) -> Self {
        Self {
            player_pub_x,
            player_pub_y,
            bet_value,
            target,
            secret_nonce,
            blind,
            asset_id,
            block_hash,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> SettleBetV1PublicInputs {
        let derived_bet_id = poseidon_hash([
            pallas::Base::from(4),
            self.player_pub_x,
            self.player_pub_y,
            self.bet_value,
            self.target,
            self.secret_nonce,
            self.blind,
            self.asset_id,
        ]);
        let roll_hash = poseidon_hash([
            pallas::Base::from(4),
            self.block_hash,
            derived_bet_id,
            self.secret_nonce,
        ]);
        SettleBetV1PublicInputs { derived_bet_id, roll_hash, tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]), tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Must match circuit witness declaration order:
            // player_pub_x, player_pub_y, bet_value, target,
            // secret_nonce, blind, asset_id, block_hash
            Witness::Base(Value::known(self.player_pub_x)),
            Witness::Base(Value::known(self.player_pub_y)),
            Witness::Base(Value::known(self.bet_value)),
            Witness::Base(Value::known(self.target)),
            Witness::Base(Value::known(self.secret_nonce)),
            Witness::Base(Value::known(self.blind)),
            Witness::Base(Value::known(self.asset_id)),
            Witness::Base(Value::known(self.block_hash)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))), // tx_binding
        ]
    }
}

/// Create a SettleBet ZK proof
pub fn create_settle_bet_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SettleBetV1CallData,
) -> Result<(Proof, SettleBetV1PublicInputs)> {
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