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

//! Capability descriptor for the Relayer Endowment contract.
//!
//! ## Capabilities
//!
//! - Relayer: initializes endowment, settles fees
//! - Backer: deploys capital, claims fees, force-settles
//!
//! Capability type discriminants:
//! - 0x00: Relayer
//! - 0x01: Backer

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

/// Capability type discriminant: Relayer.
pub const CAP_RELAYER: u8 = 0x00;
/// Capability type discriminant: Backer.
pub const CAP_BACKER: u8 = 0x01;

/// Build the full capability descriptor for the relayer_endowment contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "relayer_endowment");
    desc.actions = vec![
        // InitializeV1 (0x00): Relayer creates endowment account
        Action {
            function_id: 0x00,
            name: "Initialize".into(),
            contract_id,
            description: "Initialize a relayer endowment account".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_RELAYER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_RELAYER, b"instance").expect("valid CapabilityId derivation"),
                    description: "Active relayer endowment account".into(),
                },
            ],
        },
        // DeployCapitalV1 (0x01): Backer deploys capital
        Action {
            function_id: 0x01,
            name: "DeployCapital".into(),
            contract_id,
            description: "Deploy capital to a relayer's endowment".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_BACKER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_BACKER, b"instance").expect("valid CapabilityId derivation"),
                    description: "Active backer deployment".into(),
                },
            ],
        },
        // WithdrawDeploymentV1 (0x02): Backer withdraws deployment
        Action {
            function_id: 0x02,
            name: "WithdrawDeployment".into(),
            contract_id,
            description: "Withdraw a deployment and claim fees".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_BACKER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_BACKER, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![],
        },
        // ClaimRelayerFeesV1 (0x03): Backer claims fees
        Action {
            function_id: 0x03,
            name: "ClaimRelayerFees".into(),
            contract_id,
            description: "Claim accumulated fees from a deployment".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_BACKER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // SettleFeesV1 (0x04): Relayer settles fees to backers
        Action {
            function_id: 0x04,
            name: "SettleFees".into(),
            contract_id,
            description: "Settle fees to backer deployments".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_RELAYER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // DeactivateEndowmentV1 (0x07): Relayer deactivates endowment
        Action {
            function_id: 0x07,
            name: "DeactivateEndowment".into(),
            contract_id,
            description: "Deactivate a relayer endowment account".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_RELAYER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_RELAYER, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![],
        },
    ];
    desc
}
