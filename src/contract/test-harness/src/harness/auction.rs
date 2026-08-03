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

//! Auction Test Harness
//!
//! Provides isolated testing for Auction contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{crypto::PublicKey, pasta::pallas};
use dwow_serial::Encodable;

use dwow_auction_contract::client::{
    claim_winnings::{ClaimWinningsV1CallData, claim_winnings_v1_proof, ClaimWinningsV1PublicInputs},
    close_auction::{CloseAuctionV1CallData, close_auction_v1_proof, CloseAuctionV1PublicInputs},
    create_auction::{CreateAuctionV1CallData, create_auction_v1_proof, CreateAuctionV1PublicInputs},
    place_bid::{PlaceBidV1CallData, place_bid_v1_proof, PlaceBidV1PublicInputs},
    refund_bid::{RefundBidV1CallData, refund_bid_v1_proof, RefundBidV1PublicInputs},
    settle_auction::{SettleAuctionV1CallData, settle_auction_v1_proof, SettleAuctionV1PublicInputs},
};
use dwow_auction_contract::model::{
    CreateAuctionParamsV1, PlaceBidParamsV1, CloseAuctionParamsV1,
    ClaimWinningsParamsV1, SettleAuctionParamsV1, RefundBidParamsV1,
};

/// Auction Harness for isolated testing
pub struct AuctionHarness {
    create_auction_zkbin: ZkBinary,
    create_auction_pk: ProvingKey,
    place_bid_zkbin: ZkBinary,
    place_bid_pk: ProvingKey,
    close_auction_zkbin: ZkBinary,
    close_auction_pk: ProvingKey,
    claim_winnings_zkbin: ZkBinary,
    claim_winnings_pk: ProvingKey,
    settle_auction_zkbin: ZkBinary,
    settle_auction_pk: ProvingKey,
    refund_bid_zkbin: ZkBinary,
    refund_bid_pk: ProvingKey,
}

impl AuctionHarness {
    pub fn spawn() -> Self {
        let create_bin = include_bytes!("../../../auction/proof/create_auction.zk.bin");
        let bid_bin = include_bytes!("../../../auction/proof/place_bid.zk.bin");
        let close_bin = include_bytes!("../../../auction/proof/close_auction.zk.bin");
        let claim_bin = include_bytes!("../../../auction/proof/claim_winnings.zk.bin");
        let settle_bin = include_bytes!("../../../auction/proof/settle_auction.zk.bin");
        let refund_bin = include_bytes!("../../../auction/proof/refund_bid.zk.bin");

        let create_auction_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let place_bid_zkbin = ZkBinary::decode(bid_bin, false).unwrap();
        let close_auction_zkbin = ZkBinary::decode(close_bin, false).unwrap();
        let claim_winnings_zkbin = ZkBinary::decode(claim_bin, false).unwrap();
        let settle_auction_zkbin = ZkBinary::decode(settle_bin, false).unwrap();
        let refund_bid_zkbin = ZkBinary::decode(refund_bin, false).unwrap();

        let create_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_auction_zkbin).unwrap(),
            &create_auction_zkbin,
        );
        let bid_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&place_bid_zkbin).unwrap(),
            &place_bid_zkbin,
        );
        let close_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&close_auction_zkbin).unwrap(),
            &close_auction_zkbin,
        );
        let claim_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&claim_winnings_zkbin).unwrap(),
            &claim_winnings_zkbin,
        );
        let settle_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&settle_auction_zkbin).unwrap(),
            &settle_auction_zkbin,
        );
        let refund_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&refund_bid_zkbin).unwrap(),
            &refund_bid_zkbin,
        );

        let create_auction_pk = ProvingKey::build(create_auction_zkbin.k, &create_circuit).expect("ProvingKey::build failed");
        let place_bid_pk = ProvingKey::build(place_bid_zkbin.k, &bid_circuit).expect("ProvingKey::build failed");
        let close_auction_pk = ProvingKey::build(close_auction_zkbin.k, &close_circuit).expect("ProvingKey::build failed");
        let claim_winnings_pk = ProvingKey::build(claim_winnings_zkbin.k, &claim_circuit).expect("ProvingKey::build failed");
        let settle_auction_pk = ProvingKey::build(settle_auction_zkbin.k, &settle_circuit).expect("ProvingKey::build failed");
        let refund_bid_pk = ProvingKey::build(refund_bid_zkbin.k, &refund_circuit).expect("ProvingKey::build failed");

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

    /// Create an auction (function code 0x00)
    pub fn create_auction(
        &self,
        seller_secret: pallas::Base,
        item_commitment: pallas::Base,
        reserve_price: u64,
        token_id: pallas::Base,
        deadline_block: u64,
        current_block: u64,
        seller_public: PublicKey,
    ) -> Result<CreateAuctionResult, Box<dyn std::error::Error>> {
        let input = CreateAuctionV1CallData::new(
            seller_secret,
            item_commitment,
            pallas::Base::from(reserve_price),
            token_id,
            pallas::Base::from(deadline_block),
            pallas::Base::from(current_block),
            seller_public,
        );

        let (proof, public_inputs) = create_auction_v1_proof(
            &self.create_auction_zkbin,
            &self.create_auction_pk,
            &input,
        )?;

        let params = CreateAuctionParamsV1 {
            seller_pubkey: seller_public,
            item_commitment,
            reserve_price,
            token_id,
            deadline_block,
            auction_id: public_inputs.auction_id,
            seller_commitment: public_inputs.seller_commitment,
            merkle_proof: vec![],
            merkle_root: pallas::Base::zero(),
            instance_seed: [0u8; 32],
        };

        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode());

        Ok(CreateAuctionResult { call_data, auction_id: public_inputs.auction_id, proof, public_inputs })
    }

    /// Place a bid (function code 0x01)
    pub fn place_bid(
        &self,
        auction_id: pallas::Base,
        bidder_secret: pallas::Base,
        amount: u64,
        bid_nonce: pallas::Base,
        deadline_block: u64,
        current_block: u64,
        current_high_bid: u64,
        bidder_public: PublicKey,
    ) -> Result<PlaceBidResult, Box<dyn std::error::Error>> {
        let input = PlaceBidV1CallData::new(
            auction_id,
            bidder_secret,
            pallas::Base::from(amount),
            bid_nonce,
            pallas::Base::from(deadline_block),
            pallas::Base::from(current_block),
            pallas::Base::from(current_high_bid),
            bidder_public,
        );

        let (proof, public_inputs) = place_bid_v1_proof(
            &self.place_bid_zkbin,
            &self.place_bid_pk,
            &input,
        )?;

        let params = PlaceBidParamsV1 {
            auction_id,
            bidder_pubkey: bidder_public,
            amount,
            bid_nonce,
            bid_id: public_inputs.bid_id,
            escrow_id: pallas::Base::zero(),
            current_high_bid,
            instance_seed: [0u8; 32],
        };

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());

        Ok(PlaceBidResult { call_data, bid_id: public_inputs.bid_id, proof, public_inputs })
    }

    /// Close an auction (function code 0x02)
    pub fn close_auction(
        &self,
        auction_id: pallas::Base,
        winner_bid_id: pallas::Base,
        seller_secret: pallas::Base,
        deadline_block: u64,
        current_block: u64,
        seller_public: PublicKey,
    ) -> Result<CloseAuctionResult, Box<dyn std::error::Error>> {
        let input = CloseAuctionV1CallData::new(
            auction_id,
            winner_bid_id,
            seller_secret,
            pallas::Base::from(deadline_block),
            pallas::Base::from(current_block),
            seller_public,
        );

        let (proof, public_inputs) = close_auction_v1_proof(
            &self.close_auction_zkbin,
            &self.close_auction_pk,
            &input,
        )?;

        let params = CloseAuctionParamsV1 {
            auction_id,
            winner_bid_id,
            seller_pubkey: seller_public,
            current_block,
        };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());

        Ok(CloseAuctionResult { call_data, proof, public_inputs })
    }

    /// Claim winnings (function code 0x03)
    pub fn claim_winnings(
        &self,
        auction_id: pallas::Base,
        winner_bid_id: pallas::Base,
        winner_secret: pallas::Base,
        winner_public: PublicKey,
    ) -> Result<ClaimWinningsResult, Box<dyn std::error::Error>> {
        let input = ClaimWinningsV1CallData::new(
            auction_id,
            winner_bid_id,
            winner_secret,
            winner_public,
        );

        let (proof, public_inputs) = claim_winnings_v1_proof(
            &self.claim_winnings_zkbin,
            &self.claim_winnings_pk,
            &input,
        )?;

        let params = ClaimWinningsParamsV1 {
            auction_id,
            winner_bid_id,
            winner_pubkey: winner_public,
            winner_secret,
        };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());

        Ok(ClaimWinningsResult { call_data, proof, public_inputs })
    }

    /// Settle an auction (function code 0x04)
    pub fn settle_auction(
        &self,
        auction_id: pallas::Base,
        seller_secret: pallas::Base,
        highest_bid_amount: u64,
        seller_public: PublicKey,
    ) -> Result<SettleAuctionResult, Box<dyn std::error::Error>> {
        let input = SettleAuctionV1CallData::new(
            auction_id,
            seller_secret,
            pallas::Base::from(highest_bid_amount),
            seller_public,
        );

        let (proof, public_inputs) = settle_auction_v1_proof(
            &self.settle_auction_zkbin,
            &self.settle_auction_pk,
            &input,
        )?;

        let params = SettleAuctionParamsV1 {
            auction_id,
            seller_pubkey: seller_public,
            highest_bid_amount,
            settlement_nullifier: public_inputs.settlement_nullifier,
            seller_secret,
        };

        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());

        Ok(SettleAuctionResult { call_data, proof, public_inputs })
    }

    /// Refund a bid (function code 0x05)
    pub fn refund_bid(
        &self,
        bid_id: pallas::Base,
        bidder_secret: pallas::Base,
        bidder_public: PublicKey,
    ) -> Result<RefundBidResult, Box<dyn std::error::Error>> {
        let input = RefundBidV1CallData::new(
            bid_id,
            bidder_secret,
            bidder_public,
        );

        let (proof, public_inputs) = refund_bid_v1_proof(
            &self.refund_bid_zkbin,
            &self.refund_bid_pk,
            &input,
        )?;

        let params = RefundBidParamsV1 {
            bid_id,
            bidder_pubkey: bidder_public,
            refund_nullifier: public_inputs.refund_nullifier,
            bidder_secret,
        };

        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&params.encode());

        Ok(RefundBidResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for AuctionHarness {
    fn name(&self) -> &str {
        "auction"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateAuctionV2",
            "PlaceBidV2",
            "CloseAuctionV2",
            "ClaimWinningsV2",
            "SettleAuctionV2",
            "RefundBidV2",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateAuctionV2" => Some(&self.create_auction_zkbin),
            "PlaceBidV2" => Some(&self.place_bid_zkbin),
            "CloseAuctionV2" => Some(&self.close_auction_zkbin),
            "ClaimWinningsV2" => Some(&self.claim_winnings_zkbin),
            "SettleAuctionV2" => Some(&self.settle_auction_zkbin),
            "RefundBidV2" => Some(&self.refund_bid_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateAuctionV2" => Some(&self.create_auction_pk),
            "PlaceBidV2" => Some(&self.place_bid_pk),
            "CloseAuctionV2" => Some(&self.close_auction_pk),
            "ClaimWinningsV2" => Some(&self.claim_winnings_pk),
            "SettleAuctionV2" => Some(&self.settle_auction_pk),
            "RefundBidV2" => Some(&self.refund_bid_pk),
            _ => None,
        }
    }
}

/// Result of create_auction
pub struct CreateAuctionResult {
    pub call_data: Vec<u8>,
    pub auction_id: pallas::Base,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: CreateAuctionV1PublicInputs,
}

/// Result of place_bid
pub struct PlaceBidResult {
    pub call_data: Vec<u8>,
    pub bid_id: pallas::Base,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: PlaceBidV1PublicInputs,
}

/// Result of close_auction
pub struct CloseAuctionResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: CloseAuctionV1PublicInputs,
}

/// Result of claim_winnings
pub struct ClaimWinningsResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: ClaimWinningsV1PublicInputs,
}

/// Result of settle_auction
pub struct SettleAuctionResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: SettleAuctionV1PublicInputs,
}

/// Result of refund_bid
pub struct RefundBidResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: RefundBidV1PublicInputs,
}
