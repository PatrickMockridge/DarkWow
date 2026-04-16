/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along
 * with this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! Auction Test Harness
//!
//! Provides isolated testing for Auction contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{crypto::PublicKey, pasta::pallas};

use darkfi_auction_contract::client::{
    claim_winnings_v1::{ClaimWinningsV1CallData, claim_winnings_v1_proof},
    close_auction_v1::{CloseAuctionV1CallData, close_auction_v1_proof},
    create_auction_v1::{CreateAuctionV1CallData, create_auction_v1_proof},
    place_bid_v1::{PlaceBidV1CallData, place_bid_v1_proof},
    refund_bid_v1::{RefundBidV1CallData, refund_bid_v1_proof},
    settle_auction_v1::{SettleAuctionV1CallData, settle_auction_v1_proof},
};

/// Auction Harness for isolated testing
pub struct AuctionHarness {
    /// CreateAuction_V1 ZkBinary
    create_auction_zkbin: ZkBinary,
    /// CreateAuction_V1 ProvingKey
    create_auction_pk: ProvingKey,
    /// PlaceBid_V1 ZkBinary
    place_bid_zkbin: ZkBinary,
    /// PlaceBid_V1 ProvingKey
    place_bid_pk: ProvingKey,
    /// CloseAuction_V1 ZkBinary
    close_auction_zkbin: ZkBinary,
    /// CloseAuction_V1 ProvingKey
    close_auction_pk: ProvingKey,
    /// ClaimWinnings_V1 ZkBinary
    claim_winnings_zkbin: ZkBinary,
    /// ClaimWinnings_V1 ProvingKey
    claim_winnings_pk: ProvingKey,
    /// SettleAuction_V1 ZkBinary
    settle_auction_zkbin: ZkBinary,
    /// SettleAuction_V1 ProvingKey
    settle_auction_pk: ProvingKey,
    /// RefundBid_V1 ZkBinary
    refund_bid_zkbin: ZkBinary,
    /// RefundBid_V1 ProvingKey
    refund_bid_pk: ProvingKey,
}

impl AuctionHarness {
    /// Spawn a new Auction harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_bin = include_bytes!("../../../auction/proof/create_auction_v1.zk.bin");
        let bid_bin = include_bytes!("../../../auction/proof/place_bid_v1.zk.bin");
        let close_bin = include_bytes!("../../../auction/proof/close_auction_v1.zk.bin");
        let claim_bin = include_bytes!("../../../auction/proof/claim_winnings_v1.zk.bin");
        let settle_bin = include_bytes!("../../../auction/proof/settle_auction_v1.zk.bin");
        let refund_bin = include_bytes!("../../../auction/proof/refund_bid_v1.zk.bin");

        let create_auction_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let place_bid_zkbin = ZkBinary::decode(bid_bin, false).unwrap();
        let close_auction_zkbin = ZkBinary::decode(close_bin, false).unwrap();
        let claim_winnings_zkbin = ZkBinary::decode(claim_bin, false).unwrap();
        let settle_auction_zkbin = ZkBinary::decode(settle_bin, false).unwrap();
        let refund_bid_zkbin = ZkBinary::decode(refund_bin, false).unwrap();

        let create_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_auction_zkbin).unwrap(),
            &create_auction_zkbin,
        );
        let bid_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&place_bid_zkbin).unwrap(),
            &place_bid_zkbin,
        );
        let close_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&close_auction_zkbin).unwrap(),
            &close_auction_zkbin,
        );
        let claim_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&claim_winnings_zkbin).unwrap(),
            &claim_winnings_zkbin,
        );
        let settle_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&settle_auction_zkbin).unwrap(),
            &settle_auction_zkbin,
        );
        let refund_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&refund_bid_zkbin).unwrap(),
            &refund_bid_zkbin,
        );

        let create_auction_pk = ProvingKey::build(create_auction_zkbin.k, &create_circuit);
        let place_bid_pk = ProvingKey::build(place_bid_zkbin.k, &bid_circuit);
        let close_auction_pk = ProvingKey::build(close_auction_zkbin.k, &close_circuit);
        let claim_winnings_pk = ProvingKey::build(claim_winnings_zkbin.k, &claim_circuit);
        let settle_auction_pk = ProvingKey::build(settle_auction_zkbin.k, &settle_circuit);
        let refund_bid_pk = ProvingKey::build(refund_bid_zkbin.k, &refund_circuit);

        Self {
            create_auction_zkbin,
            create_auction_pk,
            place_bid_zkbin,
            place_bid_pk,
            close_auction_zkbin,
            close_auction_pk,
            claim_winnings_zkbin,
            claim_winnings_pk,
            settle_auction_zkbin,
            settle_auction_pk,
            refund_bid_zkbin,
            refund_bid_pk,
        }
    }

    /// Create an auction
    pub fn create_auction(
        &self,
        seller_secret: pallas::Base,
        item_commitment: pallas::Base,
        reserve_price: pallas::Base,
        token_id: pallas::Base,
        deadline_block: pallas::Base,
        current_block: pallas::Base,
        seller_public: PublicKey,
    ) -> Result<CreateAuctionResult, Box<dyn std::error::Error>> {
        let input = CreateAuctionV1CallData::new(
            seller_secret,
            item_commitment,
            reserve_price,
            token_id,
            deadline_block,
            current_block,
            seller_public,
        );

        let (proof, public_inputs) = create_auction_v1_proof(
            &self.create_auction_zkbin,
            &self.create_auction_pk,
            &input,
        )?;

        Ok(CreateAuctionResult {
            auction_id: public_inputs.auction_id,
            seller_commitment: public_inputs.seller_commitment,
            proof,
        })
    }

    /// Place a bid
    pub fn place_bid(
        &self,
        auction_id: pallas::Base,
        bidder_secret: pallas::Base,
        amount: pallas::Base,
        bid_nonce: pallas::Base,
        auction_deadline: pallas::Base,
        current_block: pallas::Base,
        bidder_public: PublicKey,
    ) -> Result<PlaceBidResult, Box<dyn std::error::Error>> {
        let input = PlaceBidV1CallData::new(
            auction_id,
            bidder_secret,
            amount,
            bid_nonce,
            auction_deadline,
            current_block,
            bidder_public,
        );

        let (proof, public_inputs) = place_bid_v1_proof(
            &self.place_bid_zkbin,
            &self.place_bid_pk,
            &input,
        )?;

        Ok(PlaceBidResult {
            bid_id: public_inputs.bid_id,
            proof,
        })
    }
}

impl super::ContractHarness for AuctionHarness {
    fn name(&self) -> &str {
        "auction"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateAuctionV1",
            "PlaceBidV1",
            "CloseAuctionV1",
            "ClaimWinningsV1",
            "SettleAuctionV1",
            "RefundBidV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateAuctionV1" => Some(&self.create_auction_zkbin),
            "PlaceBidV1" => Some(&self.place_bid_zkbin),
            "CloseAuctionV1" => Some(&self.close_auction_zkbin),
            "ClaimWinningsV1" => Some(&self.claim_winnings_zkbin),
            "SettleAuctionV1" => Some(&self.settle_auction_zkbin),
            "RefundBidV1" => Some(&self.refund_bid_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateAuctionV1" => Some(&self.create_auction_pk),
            "PlaceBidV1" => Some(&self.place_bid_pk),
            "CloseAuctionV1" => Some(&self.close_auction_pk),
            "ClaimWinningsV1" => Some(&self.claim_winnings_pk),
            "SettleAuctionV1" => Some(&self.settle_auction_pk),
            "RefundBidV1" => Some(&self.refund_bid_pk),
            _ => None,
        }
    }
}

/// Result of create_auction
pub struct CreateAuctionResult {
    pub auction_id: pallas::Base,
    pub seller_commitment: pallas::Base,
    pub proof: darkfi::zk::Proof,
}

/// Result of place_bid
pub struct PlaceBidResult {
    pub bid_id: pallas::Base,
    pub proof: darkfi::zk::Proof,
}