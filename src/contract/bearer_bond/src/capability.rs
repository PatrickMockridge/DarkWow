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

//! Capability descriptor for the Bearer Bond (Fixed-Interest Staking) contract.
//!
//! A stake coin is a tradeable capability representing a capital position.
//! Interest rights are derived deterministically from the coin. Unstaking
//! converts the coin into a receipt.
//!
//! ## Capability Types
//!
//! | Type | Discriminant | Source | Consumable |
//! |------|-------------|--------|------------|
//! | Stake Coin | 0x00 | Unspent stake in wallet | Yes |
//! | Interest Right | 0x01 | Stake coin with accrued interest | No |
//! | Unstake Right | 0x02 | Stake coin at or past maturity | No |
//! | Receipt | 0x03 | Receipt coin after unstaking | No |
//! | Emergency Unstake | 0x05 | Coverage below minimum — exit before maturity | No |

use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};
use dwow_sdk::crypto::ContractId;

/// Tradeable stake coin capability
pub const CAP_STAKE: u8 = 0x00;
/// Interest right — holder can claim deterministic interest accrued
pub const CAP_INTEREST_RIGHT: u8 = 0x01;
/// Unstake right — stake has reached maturity
pub const CAP_UNSTAKE_RIGHT: u8 = 0x02;
/// Receipt coin — proof of unstaking (non-transferable)
pub const CAP_RECEIPT: u8 = 0x03;
/// Coverage report — proof of solvency
pub const CAP_COVERAGE_REPORT: u8 = 0x04;
/// Emergency unstake right — coverage below minimum
pub const CAP_EMERGENCY_UNSTAKE: u8 = 0x05;

/// Build the capability descriptor for the Bearer Bond contract.
#[expect(clippy::expect_used, reason = "fixed ASCII instance labels always encode a canonical field element")]
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
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_STAKE, b"output").expect("valid CapabilityId derivation"),
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
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_STAKE, b"output").expect("valid CapabilityId derivation"),
                    description: "New stake coin for recipient".into(),
                },
            ],
        },
        // RequestInterestV1 (0x02): Holder requests interest payment
        Action {
            function_id: 0x02,
            name: "RequestInterestV1".into(),
            contract_id,
            description: "Request interest payment (prove ownership, provide payment key)".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance").expect("valid CapabilityId derivation"),
                CapabilityId::derive(contract_id, CAP_INTEREST_RIGHT, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_INTEREST_RIGHT, b"claim").expect("valid CapabilityId derivation"),
                    description: "Pending interest claim request".into(),
                },
            ],
        },
        // EmergencyUnstakeV1 (0x03): Exit before maturity on coverage failure
        Action {
            function_id: 0x03,
            name: "EmergencyUnstakeV1".into(),
            contract_id,
            description: "Exit before maturity when coverage falls below minimum".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance").expect("valid CapabilityId derivation"),
                CapabilityId::derive(contract_id, CAP_EMERGENCY_UNSTAKE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_RECEIPT, b"receipt").expect("valid CapabilityId derivation"),
                    description: "Receipt coin — proof of emergency unstaking".into(),
                },
            ],
        },
        // UnstakeV1 (0x04): Withdraw principal at maturity
        Action {
            function_id: 0x04,
            name: "UnstakeV1".into(),
            contract_id,
            description: "Withdraw principal + unclaimed interest at maturity".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance").expect("valid CapabilityId derivation"),
                CapabilityId::derive(contract_id, CAP_UNSTAKE_RIGHT, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_RECEIPT, b"receipt").expect("valid CapabilityId derivation"),
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
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_STAKE, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![],
        },
        // ProveCoverageV1 (0x06): Governance — prove solvency
        Action {
            function_id: 0x06,
            name: "ProveCoverageV1".into(),
            contract_id,
            description: "Prove reserves cover outstanding principal + interest obligations".into(),
            requires: CapabilityExpression::Any(vec![]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_COVERAGE_REPORT, b"report").expect("valid CapabilityId derivation"),
                    description: "Coverage report — proof of solvency".into(),
                },
            ],
        },
        // VerifyCoverageV1 (0x07): Read latest coverage report (read-only query)
        Action {
            function_id: 0x07,
            name: "VerifyCoverageV1".into(),
            contract_id,
            description: "Read latest coverage report for a series".into(),
            requires: CapabilityExpression::Any(vec![]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_COVERAGE_REPORT, b"report").expect("valid CapabilityId derivation"),
                    description: "Coverage report data".into(),
                },
            ],
        },
        // PayInterestV1 (0x08): Issuer pays a pending interest claim
        Action {
            function_id: 0x08,
            name: "PayInterestV1".into(),
            contract_id,
            description: "Pay a pending interest claim with fresh payment coin".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_COVERAGE_REPORT, b"report").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_INTEREST_RIGHT, b"claim").expect("valid CapabilityId derivation"),
            ],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_STAKE, b"output").expect("valid CapabilityId derivation"),
                    description: "Interest payment coin".into(),
                },
            ],
        },
    ];
    desc
}
