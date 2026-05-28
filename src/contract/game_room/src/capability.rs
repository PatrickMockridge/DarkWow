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

//! Capability descriptor for the Game Room contract.
//!
//! ## State Machine
//!
//! ```text
//! Open ──[Deposit]──> Active ──[PlaceBet/Raise/Call]──> (rounds continue)
//!   │                        │
//!   │                        └──[SettlePot]──> Settled ──[Claim]──> (paid out)
//!   │                        │
//!   └────────────────────────└──[ClosePot]──> Concluded
//! ```
//!
//! ## Capabilities
//!
//! - Room Owner: creates rooms, settles pots, closes pots
//! - Player: deposits, places bets, raises, calls, folds, claims winnings
//!
//! Capability type discriminants:
//! - 0x00: Room Owner
//! - 0x01: Player

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId,
};

/// Capability type discriminant: Room Owner.
pub const CAP_ROOM_OWNER: u8 = 0x00;
/// Capability type discriminant: Player.
pub const CAP_PLAYER: u8 = 0x01;

/// Build the full capability descriptor for the game room contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "game_room");
    desc.actions = vec![
        // CreateRoomV1 (0x00): Owner creates a new game room.
        Action {
            function_id: 0x00,
            name: "CreateRoom".into(),
            contract_id,
            description: "Create a new game room as the owner".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_ROOM_OWNER, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // DepositV1 (0x01): Player deposits stake.
        Action {
            function_id: 0x01,
            name: "Deposit".into(),
            contract_id,
            description: "Deposit stake into a game room as a player".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PLAYER, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // WithdrawV1 (0x02): Player withdraws stake.
        Action {
            function_id: 0x02,
            name: "Withdraw".into(),
            contract_id,
            description: "Withdraw stake from a game room as a player".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PLAYER, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // PlaceBetV1 (0x03): Player places a bet.
        Action {
            function_id: 0x03,
            name: "PlaceBet".into(),
            contract_id,
            description: "Place a bet in the current pot".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PLAYER, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // SettlePotV1 (0x08): Owner settles the pot.
        Action {
            function_id: 0x08,
            name: "SettlePot".into(),
            contract_id,
            description: "Settle the pot and distribute winnings as the owner".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_ROOM_OWNER, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // ClaimV1 (0x0A): Player claims winnings.
        Action {
            function_id: 0x0A,
            name: "Claim".into(),
            contract_id,
            description: "Claim winnings from a settled pot as a player".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PLAYER, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
