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

//! Capability descriptor for the Subscription contract.
//!
//! ## State Machine
//!
//! ```text
//!             ┌──[Renew]──> Active (extended)
//!             │
//! Subscribe ──┤
//!             │
//!             └──[VerifyAccess]──> (returns access grant)
//!
//! Active ──[Cancel]──> Cancelled
//! ```
//!
//! ## Capabilities
//!
//! - Subscriber: holds an active subscription, can verify access, renew, or cancel
//!
//! Capability type discriminants:
//! - 0x00: Active subscriber

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

/// Capability type discriminant: Active subscriber.
pub const CAP_SUBSCRIBER: u8 = 0x00;

/// Build the full capability descriptor for the subscription contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let cap_subscriber = CapabilityId::derive(contract_id, CAP_SUBSCRIBER, b"instance").expect("valid CapabilityId derivation");

    let mut desc = CapabilityDescriptor::new(contract_id, "subscription");
    desc.actions = vec![
        // SubscribeV1 (0x01): User creates a new subscription.
        Action {
            function_id: 0x01,
            name: "Subscribe".into(),
            contract_id,
            description: "Create a new subscription".into(),
            requires: CapabilityExpression::All(vec![]),
            consumes: vec![],
            produces: vec![CapabilityOutput {
                id: cap_subscriber,
                description: "Active subscriber".into(),
            }],
        },
        // CancelV1 (0x02): Subscriber cancels the subscription.
        Action {
            function_id: 0x02,
            name: "Cancel".into(),
            contract_id,
            description: "Cancel the subscription and receive refund".into(),
            requires: CapabilityExpression::All(vec![cap_subscriber]),
            consumes: vec![cap_subscriber],
            produces: vec![],
        },
        // RenewV1 (0x03): Subscriber renews the subscription.
        Action {
            function_id: 0x03,
            name: "Renew".into(),
            contract_id,
            description: "Renew the subscription for another period".into(),
            requires: CapabilityExpression::All(vec![cap_subscriber]),
            consumes: vec![],
            produces: vec![],
        },
        // VerifyAccessV1 (0x04): Subscriber proves access rights.
        Action {
            function_id: 0x04,
            name: "VerifyAccess".into(),
            contract_id,
            description: "Verify access rights for a subscription".into(),
            requires: CapabilityExpression::All(vec![cap_subscriber]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
