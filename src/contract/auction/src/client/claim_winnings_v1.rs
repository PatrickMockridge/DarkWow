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

//! Auction claim_winnings_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use rand::rngs::OsRng;

/// ClaimWinningsV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct ClaimWinningsV1PublicInputs {
    pub auction_id: pallas::Base,
    pub winner_bid_id: pallas::Base,
    pub winner_pub_x: pallas::Base,
    pub winner_pub_y: pallas::Base,
    pub tx_commitment: pallas::Base,
}

impl ClaimWinningsV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.auction_id,
            self.winner_bid_id,
            self.winner_pub_x,
            self.winner_pub_y,
            self.tx_commitment,
        ]
    }
}

/// Input data for claim_winnings proof generation
#[derive(Debug, Clone)]
pub struct ClaimWinningsV1CallData {
    pub auction_id: pallas::Base,
    pub winner_bid_id: pallas::Base,
    pub winner_secret: pallas::Base,
    // Public inputs
    pub winner_public: PublicKey,
    pub tx_commitment: pallas::Base,
}

impl ClaimWinningsV1CallData {
    pub fn new(
        auction_id: pallas::Base,
        winner_bid_id: pallas::Base,
        winner_secret: pallas::Base,
        winner_public: PublicKey,
    ) -> Self {
        Self { auction_id, winner_bid_id, winner_secret, winner_public, tx_commitment: pallas::Base::zero() }
    }

    pub fn compute_public_inputs(&self) -> ClaimWinningsV1PublicInputs {
        let (ix, iy) = self.winner_public.xy();
        ClaimWinningsV1PublicInputs {
            auction_id: self.auction_id,
            winner_bid_id: self.winner_bid_id,
            winner_pub_x: ix,
            winner_pub_y: iy,
            tx_commitment: self.tx_commitment,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.winner_public.xy();
        vec![
            // Must match circuit witness order:
            // auction_id, winner_secret, winner_bid_id, winner_pub_x, winner_pub_y
            Witness::Base(Value::known(self.auction_id)),
            Witness::Base(Value::known(self.winner_secret)),
            Witness::Base(Value::known(self.winner_bid_id)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
        ]
    }
}

/// Create a ClaimWinnings ZK proof
pub fn claim_winnings_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ClaimWinningsV1CallData,
) -> Result<(Proof, ClaimWinningsV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}