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

//! Auction place_bid_v1 ZK proof generation

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

/// PlaceBidV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct PlaceBidV1PublicInputs {
    pub auction_id: pallas::Base,
    pub bid_id: pallas::Base,
    pub amount: pallas::Base,
}

impl PlaceBidV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.auction_id, self.bid_id, self.amount]
    }
}

/// Input data for place_bid proof generation
#[derive(Debug, Clone)]
pub struct PlaceBidV1CallData {
    pub auction_id: pallas::Base,
    pub bidder_secret: pallas::Base,
    pub amount: pallas::Base,
    pub bid_nonce: pallas::Base,
    pub auction_deadline: pallas::Base,
    pub current_block: pallas::Base,
    pub current_high_bid: pallas::Base,
    // Public inputs
    pub bidder_public: PublicKey,
}

impl PlaceBidV1CallData {
    pub fn new(
        auction_id: pallas::Base,
        bidder_secret: pallas::Base,
        amount: pallas::Base,
        bid_nonce: pallas::Base,
        auction_deadline: pallas::Base,
        current_block: pallas::Base,
        current_high_bid: pallas::Base,
        bidder_public: PublicKey,
    ) -> Self {
        Self {
            auction_id,
            bidder_secret,
            amount,
            bid_nonce,
            auction_deadline,
            current_block,
            current_high_bid,
            bidder_public,
            tx_commitment: pallas::Base::zero(),
        }
    }

    /// Compute bid ID from bid parameters
    pub fn compute_bid_id(&self) -> pallas::Base {
        let (ix, iy) = self.bidder_public.xy();
        poseidon_hash([
            self.auction_id,
            ix,
            iy,
            self.amount,
            self.bid_nonce,
        ])
    }

    pub fn compute_public_inputs(&self) -> PlaceBidV1PublicInputs {
        PlaceBidV1PublicInputs {
            auction_id: self.auction_id,
            bid_id: self.compute_bid_id(),
            amount: self.amount,
            tx_commitment: self.tx_commitment,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Must match circuit witness declaration order:
            // auction_id, bidder_secret, amount, bid_nonce, auction_deadline,
            // current_block, current_high_bid
            Witness::Base(Value::known(self.auction_id)),
            Witness::Base(Value::known(self.bidder_secret)),
            Witness::Base(Value::known(self.amount)),
            Witness::Base(Value::known(self.bid_nonce)),
            Witness::Base(Value::known(self.auction_deadline)),
            Witness::Base(Value::known(self.current_block)),
            Witness::Base(Value::known(self.current_high_bid)),
        ]
    }
}

/// Create a PlaceBid ZK proof
pub fn place_bid_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PlaceBidV1CallData,
) -> Result<(Proof, PlaceBidV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}