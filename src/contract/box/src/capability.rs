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

//! Capability descriptor for the Box contract.
//!
//! Box is the ZK-native o-cap delegation primitive. It holds an arbitrary
//! capability and transfers it via linear consumption. Put creates the
//! capability; Take consumes it via nullifier.
//!
//! ## Capability Types
//!
//! | Type | Discriminant | Source | Consumable |
//! |------|-------------|--------|------------|
//! | Box Capability | 0x00 | PutV1 creates it | Yes (linear, consumed on Take) |
//!
//! ## Primitives (matching Lean4 boxCapType)
//!
//! | Primitive | Barb | Role |
//! |-----------|------|------|
//! | SecretKey | spend | Schnorr ownership proof |
//! | Nullifier | nullify | Replay prevention on Take |
//! | ContractId | dispatch | Route to BOX_CONTRACT_ID |
//! | FuncId | gate | Authorize PutV1/TakeV1 |
//! | MerkleNode | proveInclusion | Contents commitment in merkle tree |

use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};
use dwow_sdk::crypto::ContractId;

/// Box capability — linear, consumed on Take.
pub const CAP_BOX: u8 = 0x00;

/// Build the capability descriptor for the Box contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "box");
    #[expect(clippy::expect_used, reason = "fixed ASCII instance labels always encode a canonical field element")]
    let box_cap = CapabilityId::derive(contract_id, CAP_BOX, b"instance")
        .expect("valid CapabilityId derivation");

    desc.actions = vec![
        // PutV1 (0x01): Place a capability into a Box
        Action {
            function_id: 0x01,
            name: "put".into(),
            contract_id,
            description: "Place a capability into a Box — proves Box was empty".into(),
            requires: CapabilityExpression::Any(vec![]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: box_cap,
                    description: "Box capability — linear, consumed on Take".into(),
                },
            ],
        },
        // TakeV1 (0x02): Take a capability from a Box
        Action {
            function_id: 0x02,
            name: "take".into(),
            contract_id,
            description: "Take a capability from a Box — proves knowledge of box_secret, Box consumed via nullifier".into(),
            requires: CapabilityExpression::Any(vec![box_cap]),
            consumes: vec![box_cap],
            produces: vec![],
        },
    ];
    desc
}
