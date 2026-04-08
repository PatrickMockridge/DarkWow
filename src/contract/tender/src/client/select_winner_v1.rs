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

//! Tender select_winner_v1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use rand::rngs::OsRng;

/// SelectWinnerV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SelectWinnerV1PublicInputs {
    pub tender_id: pallas::Base,
    pub winner_bid_id: pallas::Base,
    pub requester_pub_x: pallas::Base,
    pub requester_pub_y: pallas::Base,
}

impl SelectWinnerV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.tender_id,
            self.winner_bid_id,
            self.requester_pub_x,
            self.requester_pub_y,
        ]
    }
}

/// Input data for select_winner proof generation
#[derive(Debug, Clone)]
pub struct SelectWinnerV1CallData {
    pub tender_id: pallas::Base,
    pub winner_bid_id: pallas::Base,
    pub requester_secret: pallas::Base,
    // Public inputs
    pub requester_public: PublicKey,
}

impl SelectWinnerV1CallData {
    pub fn new(
        tender_id: pallas::Base,
        winner_bid_id: pallas::Base,
        requester_secret: pallas::Base,
        requester_public: PublicKey,
    ) -> Self {
        Self { tender_id, winner_bid_id, requester_secret, requester_public }
    }

    pub fn compute_public_inputs(&self) -> SelectWinnerV1PublicInputs {
        let (ix, iy) = self.requester_public.xy();
        SelectWinnerV1PublicInputs {
            tender_id: self.tender_id,
            winner_bid_id: self.winner_bid_id,
            requester_pub_x: ix,
            requester_pub_y: iy,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.requester_public.xy();
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.tender_id)),
            Witness::Base(Value::known(self.winner_bid_id)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            // Private inputs
            Witness::Base(Value::known(self.requester_secret)),
        ]
    }
}

/// Create a SelectWinner ZK proof
pub fn select_winner_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SelectWinnerV1CallData,
) -> Result<(Proof, SelectWinnerV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}