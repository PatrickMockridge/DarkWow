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
//! **What reads this: nothing in this tree.** No caller of `descriptor` exists — a grep for
//! `dao_escrow_contract::capability` and for `CapabilityId::derive` outside the SDK finds no consumer —
//! so this file is a *declaration* of the intent each endpoint carries, in the same spirit as the
//! manifest's `[[capabilities]]` block, and not a check any code performs. It is kept and kept accurate
//! for that reason; a declaration that names retired endpoints is worse than no declaration, because it
//! reads as a description of the contract.
//!
//! ## The two capabilities
//!
//! - **Owner** (0x00) — the key that created the endowment. It installs the governance group and is the
//!   *only* authority while no group is installed.
//! - **Governance group** (0x01) — the MultiSig group the owner installs with `UpdateV1`. From that point
//!   on it authorises every spend and every step of the claim lifecycle.
//!
//! There used to be a third, `treasury_governor`, and a per-role capability-requirement table behind it.
//! Both are gone: nothing ever registered a requirement, so every gate that read one refused every call
//! (`OBL-C151`), and the model that carried them is what the group replaced.
//!
//! ## Reading the expressions
//!
//! `All(vec![])` — an empty conjunction — is this descriptor's way of saying **no capability is required**,
//! which is true of `initialize` (anyone may create an endowment) and of `pay_premium` (anyone may join by
//! paying). `Any([owner, group])` on `withdraw` is exact rather than loose: the endpoint has an owner path
//! and a group path, and either satisfies it.

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

/// Capability type discriminant: Owner of the endowment.
pub const CAP_OWNER: u8 = 0x00;
/// Capability type discriminant: the MultiSig group that governs the endowment.
pub const CAP_GOVERNANCE_GROUP: u8 = 0x01;

/// Build the full capability descriptor for the dao_escrow contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "dao_escrow");
    #[expect(clippy::expect_used, reason = "fixed ASCII instance_id is always a canonical field element")]
    let cap_owner = CapabilityId::derive(contract_id, CAP_OWNER, b"instance")
        .expect("valid CapabilityId derivation");
    #[expect(clippy::expect_used, reason = "fixed ASCII instance_id is always a canonical field element")]
    let cap_group = CapabilityId::derive(contract_id, CAP_GOVERNANCE_GROUP, b"instance")
        .expect("valid CapabilityId derivation");
    desc.actions = vec![
        // InitializeV1 (0x00): anyone may create an endowment; the creator becomes its owner
        Action {
            function_id: 0x00,
            name: "Initialize".into(),
            contract_id,
            description: "Create a new DAO-Escrow endowment instance".into(),
            requires: CapabilityExpression::All(vec![]),
            consumes: vec![],
            produces: vec![CapabilityOutput {
                id: cap_owner,
                description: "Owner of the DAO-Escrow instance".into(),
            }],
        },
        // UpdateV1 (0x01): the owner installs the governance group, one-shot
        Action {
            function_id: 0x01,
            name: "Update".into(),
            contract_id,
            description: "Install the MultiSig group that governs this endowment".into(),
            requires: CapabilityExpression::All(vec![cap_owner]),
            consumes: vec![],
            produces: vec![CapabilityOutput {
                id: cap_group,
                description: "Governance group of the DAO-Escrow instance".into(),
            }],
        },
        // PayPremiumV1 (0x02): anyone may join by paying at least `min_premium`
        Action {
            function_id: 0x02,
            name: "PayPremium".into(),
            contract_id,
            description: "Pay a premium to join the endowment pool".into(),
            requires: CapabilityExpression::All(vec![]),
            consumes: vec![],
            produces: vec![],
        },
        // WithdrawV1 (0x03): the owner's path, or the group's
        Action {
            function_id: 0x03,
            name: "Withdraw".into(),
            contract_id,
            description: "Withdraw from the endowment, as its owner or with the group's approval".into(),
            requires: CapabilityExpression::Any(vec![cap_owner, cap_group]),
            consumes: vec![],
            produces: vec![],
        },
        // EndowmentWithdrawV1 (0x04): a group-approved claim, escrow modes only
        Action {
            function_id: 0x04,
            name: "EndowmentWithdraw".into(),
            contract_id,
            description: "Pay a claim from the endowment, with the group's approval".into(),
            requires: CapabilityExpression::All(vec![cap_group]),
            consumes: vec![],
            produces: vec![],
        },
        // TreasurySpendV1 (0x05): a group-approved operational spend, treasury modes only
        Action {
            function_id: 0x05,
            name: "TreasurySpend".into(),
            contract_id,
            description: "Spend from the pool operationally, with the group's approval".into(),
            requires: CapabilityExpression::All(vec![cap_group]),
            consumes: vec![],
            produces: vec![],
        },
        // ProposeClaimV1 (0x07): the first step of the claim lifecycle
        Action {
            function_id: 0x07,
            name: "ProposeClaim".into(),
            contract_id,
            description: "Propose a claim against the endowment".into(),
            requires: CapabilityExpression::All(vec![cap_group]),
            consumes: vec![],
            produces: vec![],
        },
        // VoteClaimV1 (0x08): the group's decision, which is also the quorum
        Action {
            function_id: 0x08,
            name: "VoteClaim".into(),
            contract_id,
            description: "Decide a pending claim: the group's approval is the quorum".into(),
            requires: CapabilityExpression::All(vec![cap_group]),
            consumes: vec![],
            produces: vec![],
        },
        // ExecuteClaimV1 (0x09): pay out a claim the group approved
        Action {
            function_id: 0x09,
            name: "ExecuteClaim".into(),
            contract_id,
            description: "Execute a claim the group has approved".into(),
            requires: CapabilityExpression::All(vec![cap_group]),
            consumes: vec![],
            produces: vec![],
        },
        // CancelClaimV1 (0x0d): withdraw a pending proposal
        Action {
            function_id: 0x0d,
            name: "CancelClaim".into(),
            contract_id,
            description: "Cancel a pending claim, with the group's approval".into(),
            requires: CapabilityExpression::All(vec![cap_group]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
