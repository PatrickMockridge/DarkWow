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

//! Capability descriptor for the DrainProtection contract.
//!
//! ## Capabilities
//!
//! - Member: can vote, propose, transfer
//! - Guardian: can pause/unpause, trigger emergency lock
//!
//! `ExitV1` (0x04) has **no** `Action` here: the arm is refused at the dispatch pre-check
//! (`Custom(24)`, `src/contract/drain_protection/src/entrypoint.rs`), so a descriptor entry for it
//! would advertise a capability the contract will not honour. The absent entry is deliberate; a
//! reader adding it back re-opens `OBL-C102`.
//!
//! Capability type discriminants:
//! - 0x00: Member (proposer/voter)
//! - 0x01: Guardian

use dwow_sdk::crypto::ContractId;
use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};

/// Capability type discriminant: Member (proposer/voter).
pub const CAP_MEMBER: u8 = 0x00;
/// Capability type discriminant: Guardian.
pub const CAP_GUARDIAN: u8 = 0x01;

/// Build the full capability descriptor for the drain_protection contract.
#[expect(clippy::expect_used, reason = "CapabilityId::derive from short ASCII labels always yields a canonical field element")]
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "drain_protection");
    desc.actions = vec![
        // InitializeV1 (0x00): Create a new protected fund
        Action {
            function_id: 0x00,
            name: "Initialize".into(),
            contract_id,
            description: "Create a new protected fund with governance controls".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_MEMBER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_MEMBER, b"instance").expect("valid CapabilityId derivation"),
                    description: "Member of the protected fund".into(),
                },
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_GUARDIAN, b"instance").expect("valid CapabilityId derivation"),
                    description: "Guardian of the protected fund".into(),
                },
            ],
        },
        // ProposeV1 (0x01): Propose a vote
        Action {
            function_id: 0x01,
            name: "Propose".into(),
            contract_id,
            description: "Propose a governance action (withdrawal, lock, authority change)".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_MEMBER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // VoteV1 (0x02): Vote on a proposal
        Action {
            function_id: 0x02,
            name: "Vote".into(),
            contract_id,
            description: "Cast a vote on a pending proposal".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_MEMBER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // ExitV1 (0x04) is refused before any state read, so it has no `Action` — see the module
        // header. Nothing in this crate consumes `descriptor`, which is how the entry outlived the
        // refusal; the contract's own `exit_is_refused_before_any_state_read` control carries it.
        // TransferV1 (0x05): Transfer funds (rate-limited)
        Action {
            function_id: 0x05,
            name: "Transfer".into(),
            contract_id,
            description: "Transfer funds subject to rate limiting".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_MEMBER, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // LockV1 (0x06): Lock funds (guardian action)
        Action {
            function_id: 0x06,
            name: "Lock".into(),
            contract_id,
            description: "Lock funds in emergency state".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_GUARDIAN, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
        // UnlockV1 (0x07): Unlock funds
        Action {
            function_id: 0x07,
            name: "Unlock".into(),
            contract_id,
            description: "Unlock funds after timelock expires".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_GUARDIAN, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
