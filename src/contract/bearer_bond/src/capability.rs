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

//! Capability descriptor for the Bearer Bond (Profit-Share Staking) contract.
//!
//! A stake coin is a tradeable capability representing a capital position.
//! Profit rights are derived from the coin. Unstaking converts the coin
//! into a receipt.
//!
//! ## Capability Types
//!
//! | Type | Discriminant | Source | Consumable |
//! |------|-------------|--------|------------|
//! | Stake Coin | 0x00 | Unspent stake in wallet | Yes |
//! | Profit Right | 0x01 | Stake coin with unclaimed declared profits | No |
//! | Unstake Right | 0x02 | Stake coin at or past maturity | No |
//! | Receipt | 0x03 | Receipt coin after unstaking | No |

use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};
use dwow_sdk::crypto::ContractId;

/// Tradeable stake coin capability
pub const CAP_STAKE: u8 = 0x00;
/// Profit right — holder can claim pro-rata share of declared profits
pub const CAP_PROFIT_RIGHT: u8 = 0x01;
/// Unstake right — stake has reached maturity
pub const CAP_UNSTAKE_RIGHT: u8 = 0x02;
/// Receipt coin — proof of unstaking (non-transferable)
pub const CAP_RECEIPT: u8 = 0x03;
/// Coverage report — issuer proved reserves >= outstanding stake
pub const CAP_COVERAGE_REPORT: u8 = 0x04;

/// Build the capability descriptor for the Bearer Bond contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "bearer_bond");
    desc.actions = vec![
        // IssueStakeV1 (0x00): Create new staking pool
        Action {
            function_id: 0x00,
            name: "IssueStakeV1".into(),
            contract_id,
            description: "Create a new staking pool and mint initial stake coins".into(),
            requires: CapabilityExpression::Any(vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_STAKE, b"output"),
                    description: "Newly issued stake coin".into(),
                },
            ],
        },
        // TransferStakeV1 (0x01): Transfer stake position
        Action {
            function_id: 0x01,
            name: "TransferStakeV1".into(),
            contract_id,
            description: "Transfer stake position — unclaimed profits travel with the coin".into(),
            requires: CapabilityExpression::Any(vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance"),
            ],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_STAKE, b"output"),
                    description: "New stake coin for recipient".into(),
                },
            ],
        },
        // DeclareProfitsV1 (0x02): Declare profit distribution
        Action {
            function_id: 0x02,
            name: "DeclareProfitsV1".into(),
            contract_id,
            description: "Declare a profit distribution for a staking pool series".into(),
            requires: CapabilityExpression::Any(vec![]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_PROFIT_RIGHT, b"output"),
                    description: "Profit right for all stakers in the series".into(),
                },
            ],
        },
        // ClaimProfitsV1 (0x03): Claim pro-rata share
        Action {
            function_id: 0x03,
            name: "ClaimProfitsV1".into(),
            contract_id,
            description: "Claim pro-rata share of declared profits".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance"),
                CapabilityId::derive(contract_id, CAP_PROFIT_RIGHT, b"instance"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_STAKE, b"output"),
                    description: "Profit payout coin".into(),
                },
            ],
        },
        // UnstakeV1 (0x04): Withdraw principal at maturity
        Action {
            function_id: 0x04,
            name: "UnstakeV1".into(),
            contract_id,
            description: "Withdraw principal + unclaimed profits at maturity".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance"),
                CapabilityId::derive(contract_id, CAP_UNSTAKE_RIGHT, b"instance"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance"),
            ],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_RECEIPT, b"receipt"),
                    description: "Receipt coin — proof of unstaking".into(),
                },
            ],
        },
        // BurnStakeV1 (0x05): Retire staking pool
        Action {
            function_id: 0x05,
            name: "BurnStakeV1".into(),
            contract_id,
            description: "Retire staking pool — destroy remaining stake coins".into(),
            requires: CapabilityExpression::Any(vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance"),
            ],
            produces: vec![],
        },
        // ProveCoverageV1 (0x06): Governance — issuer proves solvency
        Action {
            function_id: 0x06,
            name: "ProveCoverageV1".into(),
            contract_id,
            description: "Prove issuer reserves cover outstanding stake obligations".into(),
            requires: CapabilityExpression::Any(vec![]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_COVERAGE_REPORT, b"report"),
                    description: "Coverage report — proof of solvency".into(),
                },
            ],
        },
    ];
    desc
}
