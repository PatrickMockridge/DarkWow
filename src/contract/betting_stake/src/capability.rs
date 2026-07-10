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

//! Capability descriptor for the Betting Stake contract.
//!
//! ## Capabilities
//!
//! - Staker: provides capital, earns house edge share, can unstake/claim
//!
//! Capability type discriminants:
//! - 0x00: Staker

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

/// Capability type discriminant: Staker.
pub const CAP_STAKER: u8 = 0x00;

/// Build the full capability descriptor for the betting_stake contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "betting_stake");
    desc.actions = vec![
        // StakeV1 (0x01): Staker provides capital
        Action {
            function_id: 0x01,
            name: "Stake".into(),
            contract_id,
            description: "Stake capital against a betting table".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_STAKER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_STAKER, b"instance").expect("valid CapabilityId derivation"),
                    description: "Active staker position".into(),
                },
            ],
        },
        // UnstakeV1 (0x02): Staker withdraws
        Action {
            function_id: 0x02,
            name: "Unstake".into(),
            contract_id,
            description: "Withdraw stake and accumulated earnings".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_STAKER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_STAKER, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![],
        },
        // ClaimEarningsV1 (0x03): Staker claims earnings
        Action {
            function_id: 0x03,
            name: "ClaimEarnings".into(),
            contract_id,
            description: "Claim accumulated earnings without unstaking".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_STAKER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
