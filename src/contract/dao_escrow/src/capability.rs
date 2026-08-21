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

//! Capability descriptor for the DAO-Escrow contract.
//!
//! ## Capabilities
//!
//! - Owner: creates, manages the DAO-Escrow instance
//! - Treasury governor: manages treasury operations, governance config
//!
//! Capability type discriminants:
//! - 0x00: Owner (creator/member)
//! - 0x01: Treasury governor

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

/// Capability type discriminant: Owner (creator/member).
pub const CAP_OWNER: u8 = 0x00;
/// Capability type discriminant: Treasury governor.
pub const CAP_TREASURY_GOV: u8 = 0x01;

/// Build the full capability descriptor for the dao_escrow contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "dao_escrow");
    #[expect(clippy::expect_used, reason = "fixed ASCII instance_id is always a canonical field element")]
    let cap_owner = CapabilityId::derive(contract_id, CAP_OWNER, b"instance")
        .expect("valid CapabilityId derivation");
    #[expect(clippy::expect_used, reason = "fixed ASCII instance_id is always a canonical field element")]
    let cap_treasury_gov = CapabilityId::derive(contract_id, CAP_TREASURY_GOV, b"instance")
        .expect("valid CapabilityId derivation");
    desc.actions = vec![
        // InitializeV1 (0x00): Owner creates a new DAO-Escrow instance
        Action {
            function_id: 0x00,
            name: "Initialize".into(),
            contract_id,
            description: "Create a new DAO-Escrow endowment instance".into(),
            requires: CapabilityExpression::All(vec![
                cap_owner,
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: cap_owner,
                    description: "Owner of the DAO-Escrow instance".into(),
                },
                CapabilityOutput {
                    id: cap_treasury_gov,
                    description: "Treasury governor of the DAO-Escrow instance".into(),
                },
            ],
        },
        // PayPremiumV1 (0x02): Member pays premium
        Action {
            function_id: 0x02,
            name: "PayPremium".into(),
            contract_id,
            description: "Pay premium to join the endowment pool".into(),
            requires: CapabilityExpression::All(vec![
                cap_owner,
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // WithdrawV1 (0x03): Owner withdraws from endowment
        Action {
            function_id: 0x03,
            name: "Withdraw".into(),
            contract_id,
            description: "Withdraw funds from the endowment".into(),
            requires: CapabilityExpression::All(vec![
                cap_owner,
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // EndowmentWithdrawV1 (0x04): Execute approved endowment claim
        Action {
            function_id: 0x04,
            name: "EndowmentWithdraw".into(),
            contract_id,
            description: "Execute an approved endowment withdrawal claim".into(),
            requires: CapabilityExpression::All(vec![
                cap_treasury_gov,
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // TreasurySpendV1 (0x05): Execute approved treasury spend
        Action {
            function_id: 0x05,
            name: "TreasurySpend".into(),
            contract_id,
            description: "Execute an approved treasury spend".into(),
            requires: CapabilityExpression::All(vec![
                cap_treasury_gov,
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // ProposeClaimV1 (0x07): Member proposes a claim
        Action {
            function_id: 0x07,
            name: "ProposeClaim".into(),
            contract_id,
            description: "Propose a claim against the endowment".into(),
            requires: CapabilityExpression::All(vec![
                cap_owner,
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // VoteClaimV1 (0x08): Member votes on a claim
        Action {
            function_id: 0x08,
            name: "VoteClaim".into(),
            contract_id,
            description: "Vote on a pending claim proposal".into(),
            requires: CapabilityExpression::All(vec![
                cap_owner,
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // SetGovernanceConfigV1 (0x0e): Treasury governor sets governance config
        Action {
            function_id: 0x0e,
            name: "SetGovernanceConfig".into(),
            contract_id,
            description: "Update governance configuration".into(),
            requires: CapabilityExpression::All(vec![
                cap_treasury_gov,
            ]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
