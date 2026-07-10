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

//! Capability descriptor for the Lottery contract.
//!
//! ## State Machine
//!
//! ```text
//! Initialized ──[BuyTicket]──> (tickets accumulated)
//!      │
//!      └──[DrawWinners]──> WinnersDrawn ──[ClaimPrize]──> (prizes claimed)
//!      │                          │
//!      └──[ExpireLottery]──> Expired
//! ```
//!
//! ## Capabilities
//!
//! - House: creates lotteries, draws winners, expires lotteries
//! - Player: buys tickets, reveals tickets, claims prizes
//!
//! Capability type discriminants:
//! - 0x00: House
//! - 0x01: Player / Ticket buyer

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId,
};

/// Capability type discriminant: House.
pub const CAP_HOUSE: u8 = 0x00;
/// Capability type discriminant: Player / Ticket buyer.
pub const CAP_PLAYER: u8 = 0x01;

/// Build the full capability descriptor for the lottery contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "lottery");
    desc.actions = vec![
        // InitializeV1 (0x00): House initializes a new lottery.
        Action {
            function_id: 0x00,
            name: "Initialize".into(),
            contract_id,
            description: "Initialize a new lottery round as the house".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_HOUSE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // BuyTicketV1 (0x01): Player buys a ticket.
        Action {
            function_id: 0x01,
            name: "BuyTicket".into(),
            contract_id,
            description: "Buy a lottery ticket with committed numbers".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PLAYER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // DrawWinnersV1 (0x02): House draws winning numbers.
        Action {
            function_id: 0x02,
            name: "DrawWinners".into(),
            contract_id,
            description: "Draw winning numbers as the house".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_HOUSE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // RevealTicketV1 (0x03): Player reveals ticket numbers.
        Action {
            function_id: 0x03,
            name: "RevealTicket".into(),
            contract_id,
            description: "Reveal ticket numbers to check for matches".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PLAYER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // ClaimPrizeV1 (0x04): Player claims prize.
        Action {
            function_id: 0x04,
            name: "ClaimPrize".into(),
            contract_id,
            description: "Claim prize winnings for a winning ticket".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PLAYER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // ExpireLotteryV1 (0x05): House expires lottery.
        Action {
            function_id: 0x05,
            name: "ExpireLottery".into(),
            contract_id,
            description: "Expire the lottery and claim unclaimed prizes as the house".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_HOUSE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
