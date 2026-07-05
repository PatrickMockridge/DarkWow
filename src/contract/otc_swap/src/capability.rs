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

//! Capability descriptor for the OTC Swap contract.
//!
//! Defines the capabilities a user can hold and the actions they authorize.
//!
//! ## State Machine (for reference)
//!
//! ```text
//! Created ──[Fund]──> Funded ──[Execute]──> Executed
//!    │                    │
//!    └──[Cancel]          └──[Cancel]──> Cancelled (timeout only)
//! ```
//!
//! ## Capabilities
//!
//! Each swap instance produces role-based capabilities for its participants:
//! - Alice (creator): creates, funds, cancels (Created), cancels after timeout (Funded)
//! - Bob (counterparty): executes (Funded)
//!
//! Capability type discriminants:
//! - 0x00: Alice in Created state
//! - 0x01: Bob in Created state
//! - 0x02: Alice in Funded state
//! - 0x03: Bob in Funded state

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

/// Capability type discriminant: Alice in Created state.
pub const CAP_ALICE_CREATED: u8 = 0x00;
/// Capability type discriminant: Bob in Created state.
pub const CAP_BOB_CREATED: u8 = 0x01;
/// Capability type discriminant: Alice in Funded state.
pub const CAP_ALICE_FUNDED: u8 = 0x02;
/// Capability type discriminant: Bob in Funded state.
pub const CAP_BOB_FUNDED: u8 = 0x03;

/// Build the full capability descriptor for the OTC swap contract.
///
/// The caller provides the runtime ContractId (from the wallet's contract registry)
/// so the descriptor's actions reference the correct on-chain contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "otc_swap");
    desc.actions = vec![
        // FundSwapV1 (0x02): Alice funds swap in Created state
        Action {
            function_id: 0x02,
            name: "FundSwap".into(),
            contract_id,
            description: "Lock Alice's coins into the swap".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_ALICE_CREATED, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_ALICE_FUNDED, b"instance"),
                    description: "Alice of funded swap".into(),
                },
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_BOB_FUNDED, b"instance"),
                    description: "Bob of funded swap".into(),
                },
            ],
        },
        // ExecuteSwapV1 (0x03): Bob executes swap from Funded state
        Action {
            function_id: 0x03,
            name: "ExecuteSwap".into(),
            contract_id,
            description: "Complete the swap by locking Bob's coins and releasing both".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_BOB_FUNDED, b"instance"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_ALICE_FUNDED, b"instance"),
                CapabilityId::derive(contract_id, CAP_BOB_FUNDED, b"instance"),
            ],
            produces: vec![],
        },
        // CancelSwapV1 (0x04): Alice cancels from Created or Funded (after timeout)
        // Alice always retains CAP_ALICE_CREATED even after funding (FundSwap doesn't consume it),
        // so requiring CAP_ALICE_CREATED covers both states.
        Action {
            function_id: 0x04,
            name: "CancelSwap".into(),
            contract_id,
            description: "Cancel the swap (before funding, or after timeout)".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_ALICE_CREATED, b"instance"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_ALICE_CREATED, b"instance"),
                CapabilityId::derive(contract_id, CAP_BOB_CREATED, b"instance"),
                CapabilityId::derive(contract_id, CAP_ALICE_FUNDED, b"instance"),
                CapabilityId::derive(contract_id, CAP_BOB_FUNDED, b"instance"),
            ],
            produces: vec![],
        },
    ];
    desc
}
