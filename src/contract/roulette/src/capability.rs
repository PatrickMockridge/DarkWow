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

//! Capability descriptor for the Roulette contract.
//!
//! ## State Machine
//!
//! ```text
//! Table: Active ──[SpinWheel]──> Spun ──[SettleBets]──> Settled
//!                                          │
//!                                          └──[HouseClose]──> closed
//! Bet:   placed ──[settle]──> won/lost
//! ```
//!
//! ## Capabilities
//!
//! - House: creates tables, spins wheel, settles/closes
//! - Player: places bets on tables
//!
//! Capability type discriminants:
//! - 0x00: House on a table
//! - 0x01: Player with an active bet

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId,
};

/// Capability type discriminant: House on a table.
pub const CAP_HOUSE: u8 = 0x00;
/// Capability type discriminant: Player with an active bet.
pub const CAP_PLAYER: u8 = 0x01;

/// Build the full capability descriptor for the roulette contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "roulette");
    desc.actions = vec![
        // PlaceBetV1 (0x01): Player places a bet on an active table.
        Action {
            function_id: 0x01,
            name: "PlaceBet".into(),
            contract_id,
            description: "Place a bet on a roulette table".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PLAYER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // SpinWheelV1 (0x02): House spins the wheel.
        Action {
            function_id: 0x02,
            name: "SpinWheel".into(),
            contract_id,
            description: "Spin the roulette wheel as the house".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_HOUSE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // SettleBetsV1 (0x03): House settles bets after spin.
        Action {
            function_id: 0x03,
            name: "SettleBets".into(),
            contract_id,
            description: "Settle all bets after the wheel is spun".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_HOUSE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // HouseCloseV1 (0x04): House closes the table.
        Action {
            function_id: 0x04,
            name: "HouseClose".into(),
            contract_id,
            description: "Close the roulette table as the house".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_HOUSE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
