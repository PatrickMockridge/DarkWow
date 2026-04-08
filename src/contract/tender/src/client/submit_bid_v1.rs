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

//! Tender submit_bid_v1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// SubmitBidV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SubmitBidV1PublicInputs {
    pub tender_id: pallas::Base,
    pub bid_id: pallas::Base,
    pub bidder_pub_x: pallas::Base,
    pub bidder_pub_y: pallas::Base,
}

impl SubmitBidV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.tender_id,
            self.bid_id,
            self.bidder_pub_x,
            self.bidder_pub_y,
        ]
    }
}

/// Input data for submit_bid proof generation
#[derive(Debug, Clone)]
pub struct SubmitBidV1CallData {
    pub tender_id: pallas::Base,
    pub bidder_secret: pallas::Base,
    pub amount: pallas::Base,
    pub bid_nonce: pallas::Base,
    // Public inputs
    pub bidder_public: PublicKey,
}

impl SubmitBidV1CallData {
    pub fn new(
        tender_id: pallas::Base,
        bidder_secret: pallas::Base,
        amount: pallas::Base,
        bid_nonce: pallas::Base,
        bidder_public: PublicKey,
    ) -> Self {
        Self { tender_id, bidder_secret, amount, bid_nonce, bidder_public }
    }

    /// Compute bid ID from bid parameters
    pub fn compute_bid_id(&self) -> pallas::Base {
        let (ix, iy) = self.bidder_public.xy();
        poseidon_hash([self.tender_id, ix, iy, self.amount, self.bid_nonce])
    }

    pub fn compute_public_inputs(&self) -> SubmitBidV1PublicInputs {
        let (ix, iy) = self.bidder_public.xy();
        SubmitBidV1PublicInputs {
            tender_id: self.tender_id,
            bid_id: self.compute_bid_id(),
            bidder_pub_x: ix,
            bidder_pub_y: iy,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.bidder_public.xy();
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.tender_id)),
            Witness::Base(Value::known(self.bid_id)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            // Private inputs
            Witness::Base(Value::known(self.bidder_secret)),
            Witness::Base(Value::known(self.amount)),
            Witness::Base(Value::known(self.bid_nonce)),
        ]
    }
}

/// Create a SubmitBid ZK proof
pub fn submit_bid_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SubmitBidV1CallData,
) -> Result<(Proof, SubmitBidV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}