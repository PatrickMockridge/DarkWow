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

//! Auction create_auction_v1 ZK proof generation

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

/// CreateAuctionV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreateAuctionV1PublicInputs {
    pub auction_id: pallas::Base,
    pub seller_commitment: pallas::Base,
}

impl CreateAuctionV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.auction_id, self.seller_commitment]
    }
}

/// Input data for create_auction proof generation
#[derive(Debug, Clone)]
pub struct CreateAuctionV1CallData {
    pub seller_secret: pallas::Base,
    pub item_commitment: pallas::Base,
    pub reserve_price: pallas::Base,
    pub token_id: pallas::Base,
    pub deadline_block: pallas::Base,
    pub current_block: pallas::Base,
    // Public inputs
    pub seller_public: PublicKey,
}

impl CreateAuctionV1CallData {
    pub fn new(
        seller_secret: pallas::Base,
        item_commitment: pallas::Base,
        reserve_price: pallas::Base,
        token_id: pallas::Base,
        deadline_block: pallas::Base,
        current_block: pallas::Base,
        seller_public: PublicKey,
    ) -> Self {
        Self {
            seller_secret,
            item_commitment,
            reserve_price,
            token_id,
            deadline_block,
            current_block,
            seller_public,
        }
    }

    /// Compute seller commitment from public key coordinates
    pub fn compute_seller_commitment(&self) -> pallas::Base {
        let (ix, iy) = self.seller_public.xy();
        poseidon_hash([ix, iy])
    }

    /// Compute auction ID from auction parameters
    pub fn compute_auction_id(&self) -> pallas::Base {
        let (ix, iy) = self.seller_public.xy();
        poseidon_hash([
            ix,
            iy,
            self.item_commitment,
            self.reserve_price,
            self.token_id,
            self.deadline_block,
        ])
    }

    pub fn compute_public_inputs(&self) -> CreateAuctionV1PublicInputs {
        CreateAuctionV1PublicInputs {
            auction_id: self.compute_auction_id(),
            seller_commitment: self.compute_seller_commitment(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.seller_public.xy();
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.compute_auction_id())),
            Witness::Base(Value::known(self.compute_seller_commitment())),
            // Private inputs
            Witness::Base(Value::known(self.seller_secret)),
            Witness::Base(Value::known(self.item_commitment)),
            Witness::Base(Value::known(self.reserve_price)),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(self.deadline_block)),
            Witness::Base(Value::known(self.current_block)),
        ]
    }
}

/// Create a CreateAuction ZK proof
pub fn create_auction_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateAuctionV1CallData,
) -> Result<(Proof, CreateAuctionV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}