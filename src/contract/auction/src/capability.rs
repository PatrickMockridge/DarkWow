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

//! Capability descriptor for the Auction contract.
//!
//! ## State Machine
//!
//! ```text
//! Created ──[PlaceBid]──> Active ──[Close]──> Closed ──[Settle/Claim]──> Settled
//!                                       │
//!                                       └──[RefundBid]──> bid refunded
//! ```
//!
//! ## Capabilities
//!
//! - Seller: creates, closes, settles auctions
//! - Bidder: places bids, claims winnings, requests refunds
//!
//! Capability type discriminants:
//! - 0x00: Seller of an auction
//! - 0x01: Bidder with an active bid
//! - 0x02: Bidder who was outbid

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId,
};

/// Capability type discriminant: Seller.
pub const CAP_SELLER: u8 = 0x00;
/// Capability type discriminant: Bidder with active bid.
pub const CAP_BIDDER_ACTIVE: u8 = 0x01;
/// Capability type discriminant: Outbid bidder (can refund).
pub const CAP_BIDDER_OUTBID: u8 = 0x02;

/// Build the full capability descriptor for the auction contract.
#[expect(clippy::expect_used, reason = "CapabilityId::derive from short ASCII labels always yields a canonical field element")]
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "auction");
    desc.actions = vec![
        // PlaceBidV1 (0x01): Bidder places a bid.
        Action {
            function_id: 0x01,
            name: "PlaceBid".into(),
            contract_id,
            description: "Place a bid on an auction".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_BIDDER_ACTIVE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // CloseAuctionV1 (0x02): Seller closes the auction.
        Action {
            function_id: 0x02,
            name: "CloseAuction".into(),
            contract_id,
            description: "Close the auction as the seller".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_SELLER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // ClaimWinningsV1 (0x03): Winning bidder claims item.
        Action {
            function_id: 0x03,
            name: "ClaimWinnings".into(),
            contract_id,
            description: "Claim auction winnings as the highest bidder".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_BIDDER_ACTIVE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // SettleAuctionV1 (0x04): Seller settles the auction.
        Action {
            function_id: 0x04,
            name: "SettleAuction".into(),
            contract_id,
            description: "Settle the auction and receive payment".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_SELLER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // RefundBidV1 (0x05): Outbid bidder gets refund.
        Action {
            function_id: 0x05,
            name: "RefundBid".into(),
            contract_id,
            description: "Refund an outbid bid".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_BIDDER_OUTBID, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
