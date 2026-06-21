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

//! Slot settle_bet_v1 ZK proof generation

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

/// SettleBetV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SettleBetV1PublicInputs {
    pub spin_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SettleBetV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.spin_id, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for settle_bet proof generation
#[derive(Debug, Clone)]
pub struct SettleBetV1CallData {
    pub player_pub_x: pallas::Base,
    pub player_pub_y: pallas::Base,
    pub bet_value: pallas::Base,
    pub paylines: pallas::Base,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
    pub token_id: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SettleBetV1CallData {
    pub fn new(
        player_pub: PublicKey,
        bet_value: u64,
        paylines: u32,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        token_id: pallas::Base,
    ) -> Self {
        let (px, py) = player_pub.xy();
        Self {
            player_pub_x: px,
            player_pub_y: py,
            bet_value: pallas::Base::from(bet_value),
            paylines: pallas::Base::from(paylines as u64),
            secret_nonce,
            blind,
            token_id,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> SettleBetV1PublicInputs {
        let spin_id = poseidon_hash([
            self.player_pub_x,
            self.player_pub_y,
            self.bet_value,
            self.paylines,
            self.secret_nonce,
            self.blind,
            self.token_id,
        ]);
        SettleBetV1PublicInputs { spin_id, tx_binding: pallas::Base::zero(), tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Must match circuit witness declaration order:
            // player_pub_x, player_pub_y, bet_value, paylines,
            // secret_nonce, blind, token_id, position_0, position_1,
            // position_2, match_count, payout
            Witness::Base(Value::known(self.player_pub_x)),
            Witness::Base(Value::known(self.player_pub_y)),
            Witness::Base(Value::known(self.bet_value)),
            Witness::Base(Value::known(self.paylines)),
            Witness::Base(Value::known(self.secret_nonce)),
            Witness::Base(Value::known(self.blind)),
            Witness::Base(Value::known(self.token_id)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
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
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}