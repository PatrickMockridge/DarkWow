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

//! Auction settle_auction_v1 ZK proof generation

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

/// SettleAuctionV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SettleAuctionV1PublicInputs {
    pub auction_id: pallas::Base,
    pub seller_pub_x: pallas::Base,
    pub seller_pub_y: pallas::Base,
    pub settlement_nullifier: pallas::Base,
}

impl SettleAuctionV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.auction_id,
            self.seller_pub_x,
            self.seller_pub_y,
            self.settlement_nullifier,
        ]
    }
}

/// Input data for settle_auction proof generation
#[derive(Debug, Clone)]
pub struct SettleAuctionV1CallData {
    pub auction_id: pallas::Base,
    pub seller_secret: pallas::Base,
    pub highest_bid_amount: pallas::Base,
    // Public inputs
    pub seller_public: PublicKey,
}

impl SettleAuctionV1CallData {
    pub fn new(
        auction_id: pallas::Base,
        seller_secret: pallas::Base,
        highest_bid_amount: pallas::Base,
        seller_public: PublicKey,
    ) -> Self {
        Self { auction_id, seller_secret, highest_bid_amount, seller_public, tx_commitment: pallas::Base::zero() }
    }

    /// Compute settlement nullifier from auction_id and seller_secret
    pub fn compute_settlement_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.auction_id, self.seller_secret])
    }

    pub fn compute_public_inputs(&self) -> SettleAuctionV1PublicInputs {
        let (ix, iy) = self.seller_public.xy();
        SettleAuctionV1PublicInputs {
            auction_id: self.auction_id,
            seller_pub_x: ix,
            seller_pub_y: iy,
            settlement_nullifier: self.compute_settlement_nullifier(),
            tx_commitment: self.tx_commitment,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.seller_public.xy();
        vec![
            // Must match circuit witness order:
            // auction_id, seller_secret, highest_bid_amount, seller_pub_x, seller_pub_y
            // (settlement_nullifier is computed by the circuit)
            Witness::Base(Value::known(self.auction_id)),
            Witness::Base(Value::known(self.seller_secret)),
            Witness::Base(Value::known(self.highest_bid_amount)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
        ]
    }
}

/// Create a SettleAuction ZK proof
pub fn settle_auction_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SettleAuctionV1CallData,
) -> Result<(Proof, SettleAuctionV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}