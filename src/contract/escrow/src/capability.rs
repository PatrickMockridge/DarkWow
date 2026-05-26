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

//! Capability descriptor for the Escrow contract.
//!
//! Defines the capabilities a user can hold and the actions they authorize.
//!
//! ## State Machine (for reference)
//!
//! ```text
//! Created ──[Fund]──> Funded ──[Claim]──> Claimed
//!                   │                │
//!                   │                └──[Refund]──> Refunded
//!                   │
//!                   └──[Cancel]──> Cancelled
//! ```
//!
//! ## Capabilities
//!
//! Each escrow instance produces role-based capabilities for its participants:
//! - Creator (buyer): creates, cancels (Created), refunds (Funded)
//! - Counterparty (seller): funds (Created), claims (Funded)
//!
//! The capability ID for a role capability is derived as:
//! `CapabilityId::derive(contract_id, capability_type, instance_id)`
//!
//! Capability type discriminants:
//! - 0x00: Creator in Created state
//! - 0x01: Counterparty in Created state
//! - 0x02: Creator in Funded state
//! - 0x03: Counterparty in Funded state
//! - 0x04: Creator in Claimed state (terminal)
//! - 0x05: Counterparty in Claimed state (terminal)
//! - 0x06: Creator in Refunded state (terminal)
//! - 0x07: Counterparty in Refunded state (terminal)
//! - 0x08: Creator in Cancelled state (terminal)

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

/// Capability type discriminant: Creator in Created state.
pub const CAP_CREATOR_CREATED: u8 = 0x00;
/// Capability type discriminant: Counterparty in Created state.
pub const CAP_COUNTERPARTY_CREATED: u8 = 0x01;
/// Capability type discriminant: Creator in Funded state.
pub const CAP_CREATOR_FUNDED: u8 = 0x02;
/// Capability type discriminant: Counterparty in Funded state.
pub const CAP_COUNTERPARTY_FUNDED: u8 = 0x03;

/// Build the full capability descriptor for the escrow contract.
///
/// The caller provides the runtime ContractId (from drk's contract registry)
/// so the descriptor's actions reference the correct on-chain contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "escrow");
    desc.actions = vec![
        // FundV1 (0x02): Counterparty funds escrow in Created state
        Action {
            function_id: 0x02,
            name: "FundEscrow".into(),
            contract_id,
            description: "Fund the escrow with the locked payment".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_COUNTERPARTY_CREATED, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_COUNTERPARTY_FUNDED, b"instance"),
                    description: "Counterparty of funded escrow".into(),
                },
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_CREATOR_FUNDED, b"instance"),
                    description: "Creator of funded escrow".into(),
                },
            ],
        },
        // ClaimV1 (0x03): Counterparty claims funds from Funded state
        Action {
            function_id: 0x03,
            name: "ClaimEscrow".into(),
            contract_id,
            description: "Claim the escrowed funds as the seller".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_COUNTERPARTY_FUNDED, b"instance"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_CREATOR_FUNDED, b"instance"),
                CapabilityId::derive(contract_id, CAP_COUNTERPARTY_FUNDED, b"instance"),
            ],
            produces: vec![],
        },
        // RefundV1 (0x04): Creator refunds from Funded state after timeout
        Action {
            function_id: 0x04,
            name: "RefundEscrow".into(),
            contract_id,
            description: "Refund the escrow after timeout as the buyer".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_CREATOR_FUNDED, b"instance"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_CREATOR_FUNDED, b"instance"),
                CapabilityId::derive(contract_id, CAP_COUNTERPARTY_FUNDED, b"instance"),
            ],
            produces: vec![],
        },
        // CancelV1 (0x05): Creator cancels escrow in Created state
        Action {
            function_id: 0x05,
            name: "CancelEscrow".into(),
            contract_id,
            description: "Cancel the escrow before it is funded".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_CREATOR_CREATED, b"instance"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_CREATOR_CREATED, b"instance"),
                CapabilityId::derive(contract_id, CAP_COUNTERPARTY_CREATED, b"instance"),
            ],
            produces: vec![],
        },
    ];
    desc
}
