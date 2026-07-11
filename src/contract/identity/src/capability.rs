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

//! Capability descriptor for the Identity contract.
//!
//! Identity is the credential and capability issuance primitive. It provides
//! three capability types: credential (non-consumable proof of attributes),
//! capability (consumable on verify, derived from credentials), and dag_claim
//! (composite proof through a competency DAG).
//!
//! ## Capability Types
//!
//! | Type | Discriminant | Source | Consumable |
//! |------|-------------|--------|------------|
//! | Credential | 0x00 | IssueCredentialV1 | No |
//! | Capability | 0x01 | IssueCapabilityV1 | Yes (consumed on verify) |
//! | DAG Claim | 0x02 | CreateClaim_DAG_V1 | No |

use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};
use dwow_sdk::crypto::ContractId;

pub const CAP_CREDENTIAL: u8 = 0x00;
pub const CAP_CAPABILITY: u8 = 0x01;
pub const CAP_DAG_CLAIM: u8 = 0x02;

pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "identity");
    let cap_credential = CapabilityId::derive(contract_id, CAP_CREDENTIAL, b"instance")
        .expect("valid CapabilityId derivation");
    let cap_capability = CapabilityId::derive(contract_id, CAP_CAPABILITY, b"instance")
        .expect("valid CapabilityId derivation");
    let cap_dag = CapabilityId::derive(contract_id, CAP_DAG_CLAIM, b"instance")
        .expect("valid CapabilityId derivation");

    desc.actions = vec![
        Action {
            function_id: 0x00,
            name: "IssueCredentialV1".into(),
            contract_id,
            description: "Issue a credential proving the holder possesses an attribute".into(),
            requires: CapabilityExpression::Any(vec![]),
            consumes: vec![],
            produces: vec![CapabilityOutput { id: cap_credential, description: "New credential".into() }],
        },
        Action {
            function_id: 0x02,
            name: "IssueCapabilityV1".into(),
            contract_id,
            description: "Issue a registered capability derived from a credential".into(),
            requires: CapabilityExpression::All(vec![cap_credential]),
            consumes: vec![],
            produces: vec![CapabilityOutput { id: cap_capability, description: "New capability".into() }],
        },
        Action {
            function_id: 0x04,
            name: "VerifyCapabilityV1".into(),
            contract_id,
            description: "Verify a capability — proves possession via Box Take".into(),
            requires: CapabilityExpression::Any(vec![cap_capability]),
            consumes: vec![cap_capability],
            produces: vec![],
        },
        Action {
            function_id: 0x0A,
            name: "CreateClaim_DAG_V1".into(),
            contract_id,
            description: "Create a composite DAG claim proving competency path".into(),
            requires: CapabilityExpression::Any(vec![]),
            consumes: vec![],
            produces: vec![CapabilityOutput { id: cap_dag, description: "New DAG claim".into() }],
        },
    ];
    desc
}
