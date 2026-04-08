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

//! Auction close_auction_v1 ZK proof generation

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

/// CloseAuctionV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CloseAuctionV1PublicInputs {
    pub auction_id: pallas::Base,
    pub winner_bid_id: pallas::Base,
    pub seller_pub_x: pallas::Base,
    pub seller_pub_y: pallas::Base,
}

impl CloseAuctionV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.auction_id,
            self.winner_bid_id,
            self.seller_pub_x,
            self.seller_pub_y,
        ]
    }
}

/// Input data for close_auction proof generation
#[derive(Debug, Clone)]
pub struct CloseAuctionV1CallData {
    pub auction_id: pallas::Base,
    pub winner_bid_id: pallas::Base,
    pub seller_secret: pallas::Base,
    pub auction_deadline: pallas::Base,
    pub current_block: pallas::Base,
    // Public inputs
    pub seller_public: PublicKey,
}

impl CloseAuctionV1CallData {
    pub fn new(
        auction_id: pallas::Base,
        winner_bid_id: pallas::Base,
        seller_secret: pallas::Base,
        auction_deadline: pallas::Base,
        current_block: pallas::Base,
        seller_public: PublicKey,
    ) -> Self {
        Self {
            auction_id,
            winner_bid_id,
            seller_secret,
            auction_deadline,
            current_block,
            seller_public,
        }
    }

    pub fn compute_public_inputs(&self) -> CloseAuctionV1PublicInputs {
        let (ix, iy) = self.seller_public.xy();
        CloseAuctionV1PublicInputs {
            auction_id: self.auction_id,
            winner_bid_id: self.winner_bid_id,
            seller_pub_x: ix,
            seller_pub_y: iy,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.seller_public.xy();
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.auction_id)),
            Witness::Base(Value::known(self.winner_bid_id)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            // Private inputs
            Witness::Base(Value::known(self.seller_secret)),
            Witness::Base(Value::known(self.winner_bid_id)),
            Witness::Base(Value::known(self.auction_deadline)),
            Witness::Base(Value::known(self.current_block)),
        ]
    }
}

/// Create a CloseAuction ZK proof
pub fn close_auction_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CloseAuctionV1CallData,
) -> Result<(Proof, CloseAuctionV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}