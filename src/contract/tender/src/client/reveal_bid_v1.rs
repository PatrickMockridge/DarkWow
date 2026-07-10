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

//! Tender reveal_bid_v1 ZK proof generation

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

/// RevealBidV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct RevealBidV1PublicInputs {
    pub tender_id: pallas::Base,
    pub bid_id: pallas::Base,
    pub revealed_amount: pallas::Base,
    pub bidder_pub_x: pallas::Base,
    pub bidder_pub_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevealBidV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.tender_id,
            self.bid_id,
            self.revealed_amount,
            self.bidder_pub_x,
            self.bidder_pub_y,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for reveal_bid proof generation
#[derive(Debug, Clone)]
pub struct RevealBidV1CallData {
    pub tender_id: pallas::Base,
    pub bid_id: pallas::Base,
    pub bidder_secret: pallas::Base,
    pub revealed_amount: pallas::Base,
    // Public inputs
    pub bidder_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevealBidV1CallData {
    pub fn new(
        tender_id: pallas::Base,
        bid_id: pallas::Base,
        bidder_secret: pallas::Base,
        revealed_amount: pallas::Base,
        bidder_public: PublicKey,
    ) -> Self {
        Self {
            tender_id,
            bid_id,
            bidder_secret,
            revealed_amount,
            bidder_public,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> RevealBidV1PublicInputs {
        let (ix, iy) = self.bidder_public.xy().expect("pk not identity");
        RevealBidV1PublicInputs {
            tender_id: self.tender_id,
            bid_id: self.bid_id,
            revealed_amount: self.revealed_amount,
            bidder_pub_x: ix,
            bidder_pub_y: iy,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.bidder_public.xy().expect("pk not identity");
        vec![
            // Must match circuit witness order:
            // tender_id, bid_id, bidder_secret, bidder_pub_x, bidder_pub_y, revealed_amount
            Witness::Base(Value::known(self.tender_id)),
            Witness::Base(Value::known(self.bid_id)),
            Witness::Base(Value::known(self.bidder_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.revealed_amount)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a RevealBid ZK proof
pub fn reveal_bid_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RevealBidV1CallData,
) -> Result<(Proof, RevealBidV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}