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

//! Capability descriptor for the Pool Stake contract.
//!
//! ## Capabilities
//!
//! - Pool Creator: creates and manages pools
//! - Pool Member: stakes, allocates coverage, claims fees
//!
//! Capability type discriminants:
//! - 0x00: Pool Creator
//! - 0x01: Pool Member

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

/// Capability type discriminant: Pool Creator.
pub const CAP_POOL_CREATOR: u8 = 0x00;
/// Capability type discriminant: Pool Member.
pub const CAP_POOL_MEMBER: u8 = 0x01;

/// Build the full capability descriptor for the pool_stake contract.
#[expect(clippy::expect_used, reason = "CapabilityId::derive is infallible for fixed ASCII instance labels")]
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "pool_stake");
    desc.actions = vec![
        // CreatePoolV1 (0x00): Creator creates a pool
        Action {
            function_id: 0x00,
            name: "CreatePool".into(),
            contract_id,
            description: "Create a new staking pool".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_POOL_CREATOR, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_POOL_CREATOR, b"instance").expect("valid CapabilityId derivation"),
                    description: "Pool creator of a staking pool".into(),
                },
            ],
        },
        // JoinPoolV1 (0x01): Member joins a pool
        Action {
            function_id: 0x01,
            name: "JoinPool".into(),
            contract_id,
            description: "Join an existing pool by staking capital".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_POOL_MEMBER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_POOL_MEMBER, b"instance").expect("valid CapabilityId derivation"),
                    description: "Active pool member position".into(),
                },
            ],
        },
        // LeavePoolV1 (0x02): Member leaves a pool
        Action {
            function_id: 0x02,
            name: "LeavePool".into(),
            contract_id,
            description: "Leave a pool after cooldown period".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_POOL_MEMBER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_POOL_MEMBER, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![],
        },
        // AllocateCoverageV1 (0x03): Member allocates coverage
        Action {
            function_id: 0x03,
            name: "AllocateCoverage".into(),
            contract_id,
            description: "Allocate coverage for a guaranteed withdrawal".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_POOL_MEMBER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
