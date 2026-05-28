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

//! Capability descriptor for the Darkbet Exchange contract.
//!
//! Capability type discriminants:
//! - 0x00: Market Creator
//! - 0x01: Backer
//! - 0x02: Layer
//! - 0x03: LP Provider
//! - 0x04: Oracle

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

pub const CAP_CREATOR: u8 = 0x00;
pub const CAP_BACKER: u8 = 0x01;
pub const CAP_LAYER: u8 = 0x02;
pub const CAP_LP_PROVIDER: u8 = 0x03;
pub const CAP_ORACLE: u8 = 0x04;

pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "darkbet_exchange");
    desc.actions = vec![
        Action {
            function_id: 0x00,
            name: "CreateMarket".into(),
            contract_id,
            description: "Create a new prediction market".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_CREATOR, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_CREATOR, b"instance"),
                    description: "Creator of the market".into(),
                },
            ],
        },
        Action {
            function_id: 0x01,
            name: "PlaceBack".into(),
            contract_id,
            description: "Place a back (for) bet on a market".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_BACKER, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        Action {
            function_id: 0x02,
            name: "PlaceLay".into(),
            contract_id,
            description: "Place a lay (against) bet on a market".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_LAYER, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        Action {
            function_id: 0x04,
            name: "ResolveMarket".into(),
            contract_id,
            description: "Resolve a market outcome (oracle)".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_ORACLE, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        Action {
            function_id: 0x08,
            name: "AddLiquidity".into(),
            contract_id,
            description: "Add liquidity to the market".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_LP_PROVIDER, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        Action {
            function_id: 0x0A,
            name: "ClaimWinnings".into(),
            contract_id,
            description: "Claim winnings from a settled market".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_BACKER, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
