/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! DarkToshi Dice commit_bet_v1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{pedersen_commitment_u64, poseidon_hash, Blind, PublicKey},
    pasta::pallas,
};
use darkfi_sdk::crypto::pasta_prelude::*;
use rand::rngs::OsRng;

/// CommitBetV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CommitBetV1PublicInputs {
    pub bet_id: pallas::Base,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
}

impl CommitBetV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.bet_id, self.value_commit_x, self.value_commit_y]
    }
}

/// Input data for commit_bet proof generation
#[derive(Debug, Clone)]
pub struct CommitBetV1CallData {
    pub player_pub_x: pallas::Base,
    pub player_pub_y: pallas::Base,
    pub bet_value: u64,
    pub target: pallas::Base,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
    pub token_id: pallas::Base,
    pub house_edge: pallas::Base,
    pub value_blind: pallas::Scalar,
}

impl CommitBetV1CallData {
    pub fn new(
        player_pub: PublicKey,
        bet_value: u64,
        target: u8,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        token_id: pallas::Base,
        house_edge: u32,
        value_blind: pallas::Scalar,
    ) -> Self {
        let (px, py) = player_pub.xy();
        Self {
            player_pub_x: px,
            player_pub_y: py,
            bet_value,
            target: pallas::Base::from(target as u64),
            secret_nonce,
            blind,
            token_id,
            house_edge: pallas::Base::from(house_edge as u64),
            value_blind,
        }
    }

    pub fn compute_public_inputs(&self) -> CommitBetV1PublicInputs {
        let bet_id = poseidon_hash([
            self.player_pub_x,
            self.player_pub_y,
            pallas::Base::from(self.bet_value),
            self.target,
            self.secret_nonce,
            self.blind,
            self.token_id,
        ]);
        // Compute value commitment: vcv = bet_value * G1 + value_blind * G2
        let value_commit =
            pedersen_commitment_u64(self.bet_value, Blind(self.value_blind));
        let coords = value_commit.to_affine().coordinates().unwrap();
        CommitBetV1PublicInputs {
            bet_id,
            value_commit_x: *coords.x(),
            value_commit_y: *coords.y(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Witnesses (must match circuit order)
            Witness::Base(Value::known(self.player_pub_x)),
            Witness::Base(Value::known(self.player_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.bet_value))),
            Witness::Base(Value::known(self.target)),
            Witness::Base(Value::known(self.secret_nonce)),
            Witness::Base(Value::known(self.blind)),
            Witness::Base(Value::known(self.token_id)),
            Witness::Scalar(Value::known(self.value_blind)),
        ]
    }
}

/// Create a CommitBet ZK proof
pub fn create_commit_bet_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CommitBetV1CallData,
) -> Result<(Proof, CommitBetV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}