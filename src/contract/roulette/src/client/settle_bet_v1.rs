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

//! Roulette settle_bet_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::pasta::pallas;
use rand::rngs::OsRng;

/// SettleBetV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SettleBetV1PublicInputs {
    pub payout: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SettleBetV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.payout, self.tx_binding,
            self.tx_nonce]
    }
}

/// Input data for settle_bet proof generation
#[derive(Debug, Clone)]
pub struct SettleBetV1CallData {
    pub table_id: pallas::Base,
    pub bet_id: pallas::Base,
    pub won: pallas::Base,
    pub payout: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SettleBetV1CallData {
    pub fn new(table_id: pallas::Base, bet_id: pallas::Base, won: bool, payout: u64) -> Self {
        Self {
            table_id,
            bet_id,
            won: if won { pallas::Base::one() } else { pallas::Base::zero() },
            payout,
            tx_commitment: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> SettleBetV1PublicInputs {
        SettleBetV1PublicInputs { payout: pallas::Base::from(self.payout), tx_binding: poseidon_hash([self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Witnesses (must match circuit order)
            Witness::Base(Value::known(self.table_id)),
            Witness::Base(Value::known(self.bet_id)),
            Witness::Base(Value::known(self.won)),
            Witness::Base(Value::known(pallas::Base::from(self.payout))),
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