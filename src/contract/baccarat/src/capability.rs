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

//! Capability descriptor for the Baccarat contract.
//!
//! ## State Machine
//!
//! ```text
//! Committed ──[DrawCards]──> CardsDrawn ──[Settle]──> Settled
//!      │                         │
//!      └──[HouseClose]───> Cancelled
//!                           (also from CardsDrawn after timeout)
//! ```
//!
//! ## Capabilities
//!
//! - Player: the bettor, identified by `player_pub` on the Bet
//! - House: the contract house, identified by stored `house_pubkey`
//!
//! Capability type discriminants:
//! - 0x00: Player in Committed state
//! - 0x01: Player in CardsDrawn state
//! - 0x02: House

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

/// Capability type discriminant: Player in Committed state.
pub const CAP_PLAYER_COMMITTED: u8 = 0x00;
/// Capability type discriminant: Player in CardsDrawn state.
pub const CAP_PLAYER_CARDS_DRAWN: u8 = 0x01;
/// Capability type discriminant: House (global role).
pub const CAP_HOUSE: u8 = 0x02;

/// Build the full capability descriptor for the baccarat contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "baccarat");
    desc.actions = vec![
        // DrawCardsV1 (0x02): Anyone who knows secret_nonce can trigger;
        // capability-wise it's the Player advancing from Committed to CardsDrawn.
        Action {
            function_id: 0x02,
            name: "DrawCards".into(),
            contract_id,
            description: "Draw cards for the bet using block hash entropy".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PLAYER_COMMITTED, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_PLAYER_CARDS_DRAWN, b"instance").expect("valid CapabilityId derivation"),
                    description: "Player of a bet with cards drawn".into(),
                },
            ],
        },
        // SettleBetV1 (0x03): Player settles the bet after cards are drawn.
        Action {
            function_id: 0x03,
            name: "SettleBet".into(),
            contract_id,
            description: "Settle the bet and receive payout".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PLAYER_CARDS_DRAWN, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_PLAYER_CARDS_DRAWN, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![],
        },
        // HouseCloseV1 (0x04): House closes an expired/unsettled bet.
        Action {
            function_id: 0x04,
            name: "HouseClose".into(),
            contract_id,
            description: "Close an expired or unsettled bet as the house".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_HOUSE, b"house").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_PLAYER_COMMITTED, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![],
        },
    ];
    desc
}
