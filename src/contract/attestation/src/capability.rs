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

//! Capability descriptor for the Attestation contract.
//!
//! Attestation is the trust verification primitive. It provides three capability
//! types: attestation (on-chain attestation from a trusted issuer), claim (a
//! claim against an attestation, consumable on verify), and delegation (delegated
//! attestation authority, non-consumable, revocable).
//!
//! ## Capability Types
//!
//! | Type | Discriminant | Source | Consumable |
//! |------|-------------|--------|------------|
//! | Attestation | 0x00 | CreateAttestationV1 | No |
//! | Claim | 0x01 | CreateClaimV1 | Yes (consumed on verify) |
//! | Delegation | 0x02 | DelegateAttestationV1 | No (revocable) |

use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};
use dwow_sdk::crypto::ContractId;

/// On-chain attestation from a trusted issuer.
pub const CAP_ATTESTATION: u8 = 0x00;
/// Claim against an attestation — consumable on verify.
pub const CAP_CLAIM: u8 = 0x01;
/// Delegated attestation authority — non-consumable, revocable.
pub const CAP_DELEGATION: u8 = 0x02;

/// Build the capability descriptor for the Attestation contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "attestation");
    #[expect(clippy::expect_used, reason = "fixed ASCII instance_id is always a canonical field element")]
    let cap_attestation = CapabilityId::derive(contract_id, CAP_ATTESTATION, b"instance")
        .expect("valid CapabilityId derivation");
    #[expect(clippy::expect_used, reason = "fixed ASCII instance_id is always a canonical field element")]
    let cap_claim = CapabilityId::derive(contract_id, CAP_CLAIM, b"instance")
        .expect("valid CapabilityId derivation");
    #[expect(clippy::expect_used, reason = "fixed ASCII instance_id is always a canonical field element")]
    let cap_delegation = CapabilityId::derive(contract_id, CAP_DELEGATION, b"instance")
        .expect("valid CapabilityId derivation");

    desc.actions = vec![
        Action {
            function_id: 0x00,
            name: "CreateAttestationV1".into(),
            contract_id,
            description: "Create an on-chain attestation from a trusted issuer".into(),
            requires: CapabilityExpression::Any(vec![]),
            consumes: vec![],
            produces: vec![CapabilityOutput { id: cap_attestation, description: "New attestation".into() }],
        },
        Action {
            function_id: 0x03,
            name: "CreateClaimV1".into(),
            contract_id,
            description: "Create a claim against an attestation".into(),
            requires: CapabilityExpression::Any(vec![cap_attestation]),
            consumes: vec![],
            produces: vec![CapabilityOutput { id: cap_claim, description: "New claim".into() }],
        },
        Action {
            function_id: 0x05,
            name: "ConsumeClaimV1".into(),
            contract_id,
            description: "Consume a claim — prevents replay via nullifier".into(),
            requires: CapabilityExpression::Any(vec![cap_claim]),
            consumes: vec![cap_claim],
            produces: vec![],
        },
        Action {
            function_id: 0x08,
            name: "DelegateAttestationV1".into(),
            contract_id,
            description: "Delegate attestation authority to another party".into(),
            requires: CapabilityExpression::Any(vec![cap_attestation]),
            consumes: vec![],
            produces: vec![CapabilityOutput { id: cap_delegation, description: "New delegation".into() }],
        },
    ];
    desc
}
