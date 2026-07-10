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

//! Capability descriptor for the DEX contract.
//!
//! The DEX uses per-swap ephemeral signature keys (derived via
//! `SecretKey::derive_instance`), so each swap naturally produces
//! unlinkable pubkeys on-chain.
//!
//! ## Capabilities
//!
//! Each swap instance produces role-based capabilities for its participants:
//! - Proposer: creates the swap, can cancel
//! - Acceptor: accepts the swap, locking matching funds
//!
//! Capability type discriminants:
//! - 0x00: Proposer (swap creator)
//! - 0x01: Acceptor (swap acceptor)

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId,
};

/// Capability type discriminant: Proposer.
pub const CAP_PROPOSER: u8 = 0x00;
/// Capability type discriminant: Acceptor.
pub const CAP_ACCEPTOR: u8 = 0x01;

pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "dex");
    desc.actions = vec![
        // CreateSwapV1 (0x01): Proposer creates a swap
        Action {
            function_id: 0x01,
            name: "CreateSwap".into(),
            contract_id,
            description: "Create a new atomic swap proposal".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PROPOSER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // AcceptSwapV1 (0x02): Acceptor accepts a swap
        Action {
            function_id: 0x02,
            name: "AcceptSwap".into(),
            contract_id,
            description: "Accept an existing swap proposal".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_ACCEPTOR, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // CancelSwapV1 (0x04): Proposer cancels a swap
        Action {
            function_id: 0x04,
            name: "CancelSwap".into(),
            contract_id,
            description: "Cancel a pending swap proposal".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_PROPOSER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
