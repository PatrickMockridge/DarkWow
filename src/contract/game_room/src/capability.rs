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
//!                 ┌──[Raise/Call]──> (round continues)
//!                 │
//! Open ──[Dep]──> Active ──[Fold]──> Folded (consumed)
//!   │               │
//!   │               └──[SettlePot]──> Settled ──[Claim]──> (paid out, consumed)
//!   │               │
//!   └───────────────└──[ClosePot]──> Concluded
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
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

/// Capability type discriminant: Room Owner.
pub const CAP_ROOM_OWNER: u8 = 0x00;
/// Capability type discriminant: Player.
pub const CAP_PLAYER: u8 = 0x01;

/// Build the full capability descriptor for the game room contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let cap_room_owner = CapabilityId::derive(contract_id, CAP_ROOM_OWNER, b"instance");
    let cap_player = CapabilityId::derive(contract_id, CAP_PLAYER, b"instance");

    let mut desc = CapabilityDescriptor::new(contract_id, "game_room");
    desc.actions = vec![
        // CreateRoomV1 (0x00): Owner creates a new game room.
        Action {
            function_id: 0x00,
            name: "CreateRoom".into(),
            contract_id,
            description: "Create a new game room as the owner".into(),
            requires: CapabilityExpression::All(vec![cap_room_owner]),
            consumes: vec![],
            produces: vec![CapabilityOutput {
                id: cap_room_owner,
                description: "Room owner".into(),
            }],
        },
        // DepositV1 (0x01): Player deposits stake and becomes active.
        Action {
            function_id: 0x01,
            name: "Deposit".into(),
            contract_id,
            description: "Deposit stake into a game room as a player".into(),
            requires: CapabilityExpression::All(vec![]),
            consumes: vec![],
            produces: vec![CapabilityOutput {
                id: cap_player,
                description: "Active player".into(),
            }],
        },
        // WithdrawV1 (0x02): Player withdraws stake (consumes player capability).
        Action {
            function_id: 0x02,
            name: "Withdraw".into(),
            contract_id,
            description: "Withdraw stake and leave the game room".into(),
            requires: CapabilityExpression::All(vec![cap_player]),
            consumes: vec![cap_player],
            produces: vec![],
        },
        // PlaceBetV1 (0x03): Player places a bet (retains active status).
        Action {
            function_id: 0x03,
            name: "PlaceBet".into(),
            contract_id,
            description: "Place a bet in the current pot".into(),
            requires: CapabilityExpression::All(vec![cap_player]),
            consumes: vec![],
            produces: vec![],
        },
        // RaiseV1 (0x04): Player raises (retains active status).
        Action {
            function_id: 0x04,
            name: "Raise".into(),
            contract_id,
            description: "Raise the current bet".into(),
            requires: CapabilityExpression::All(vec![cap_player]),
            consumes: vec![],
            produces: vec![],
        },
        // CallV1 (0x05): Player calls (retains active status).
        Action {
            function_id: 0x05,
            name: "Call".into(),
            contract_id,
            description: "Call the current bet".into(),
            requires: CapabilityExpression::All(vec![cap_player]),
            consumes: vec![],
            produces: vec![],
        },
        // FoldV1 (0x06): Player folds (consumes active player capability).
        Action {
            function_id: 0x06,
            name: "Fold".into(),
            contract_id,
            description: "Fold and forfeit the current hand".into(),
            requires: CapabilityExpression::All(vec![cap_player]),
            consumes: vec![cap_player],
            produces: vec![],
        },
        // ClosePotV1 (0x07): Owner closes the pot.
        Action {
            function_id: 0x07,
            name: "ClosePot".into(),
            contract_id,
            description: "Close the pot and conclude the game as the owner".into(),
            requires: CapabilityExpression::All(vec![cap_room_owner]),
            consumes: vec![cap_room_owner],
            produces: vec![],
        },
        // SettlePotV1 (0x08): Owner settles the pot.
        Action {
            function_id: 0x08,
            name: "SettlePot".into(),
            contract_id,
            description: "Settle the pot and distribute winnings as the owner".into(),
            requires: CapabilityExpression::All(vec![cap_room_owner]),
            consumes: vec![],
            produces: vec![],
        },
        // ContributeEntropyV1 (0x09): Player contributes entropy.
        Action {
            function_id: 0x09,
            name: "ContributeEntropy".into(),
            contract_id,
            description: "Contribute entropy for randomness (commit-reveal)".into(),
            requires: CapabilityExpression::All(vec![cap_player]),
            consumes: vec![],
            produces: vec![],
        },
        // ClaimV1 (0x0A): Player claims winnings (consumes player capability).
        Action {
            function_id: 0x0A,
            name: "Claim".into(),
            contract_id,
            description: "Claim winnings from a settled pot as a player".into(),
            requires: CapabilityExpression::All(vec![cap_player]),
            consumes: vec![cap_player],
            produces: vec![],
        },
    ];
    desc
}
