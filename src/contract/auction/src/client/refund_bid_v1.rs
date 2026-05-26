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

//! Auction refund_bid_v1 ZK proof generation

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

/// RefundBidV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct RefundBidV1PublicInputs {
    pub bid_id: pallas::Base,
    pub bidder_pub_x: pallas::Base,
    pub bidder_pub_y: pallas::Base,
    pub refund_nullifier: pallas::Base,
}

impl RefundBidV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.bid_id,
            self.bidder_pub_x,
            self.bidder_pub_y,
            self.refund_nullifier,
        ]
    }
}

/// Input data for refund_bid proof generation
#[derive(Debug, Clone)]
pub struct RefundBidV1CallData {
    pub bid_id: pallas::Base,
    pub bidder_secret: pallas::Base,
    // Public inputs
    pub bidder_public: PublicKey,
}

impl RefundBidV1CallData {
    pub fn new(bid_id: pallas::Base, bidder_secret: pallas::Base, bidder_public: PublicKey) -> Self {
        Self { bid_id, bidder_secret, bidder_public }
    }

    /// Compute refund nullifier from bid_id and bidder_secret
    pub fn compute_refund_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.bid_id, self.bidder_secret])
    }

    pub fn compute_public_inputs(&self) -> RefundBidV1PublicInputs {
        let (ix, iy) = self.bidder_public.xy();
        RefundBidV1PublicInputs {
            bid_id: self.bid_id,
            bidder_pub_x: ix,
            bidder_pub_y: iy,
            refund_nullifier: self.compute_refund_nullifier(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.bidder_public.xy();
        vec![
            // Must match circuit witness order:
            // bid_id, bidder_secret, bidder_pub_x, bidder_pub_y
            // (refund_nullifier is computed by the circuit)
            Witness::Base(Value::known(self.bid_id)),
            Witness::Base(Value::known(self.bidder_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
        ]
    }
}

/// Create a RefundBid ZK proof
pub fn refund_bid_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RefundBidV1CallData,
) -> Result<(Proof, RefundBidV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}