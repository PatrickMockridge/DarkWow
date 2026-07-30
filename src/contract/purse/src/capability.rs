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

//! Capability descriptor for the Purse contract.
//!
//! Purse is the ZK-native o-cap value store primitive. It holds a token balance
//! as a linear capability. Deposit creates the capability with an initial
//! balance; Withdraw consumes the current capability and produces a new one
//! with reduced balance; Balance proves knowledge of a purse's balance
//! without consuming it.
//!
//! ## Capability Types
//!
//! | Type | Discriminant | Source | Consumable |
//! |------|-------------|--------|------------|
//! | Purse Capability | 0x00 | DepositV1 creates it | Yes (consumed + re-produced on Withdraw) |
//!
//! ## Primitives (matching Lean4 purseCapType)
//!
//! | Primitive | Barb | Role |
//! |-----------|------|------|
//! | SecretKey | spend | Schnorr ownership proof |
//! | Nullifier | nullify | Replay prevention on Withdraw |
//! | ContractId | dispatch | Route to PURSE_CONTRACT_ID |
//! | FuncId | gate | Authorize DepositV1/WithdrawV1/BalanceV1 |
//! | MerkleNode | proveInclusion | Balance commitment in merkle tree |
//! | PedersenCommitment | hide | Balance hiding via Pedersen commitments |

use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};
use dwow_sdk::crypto::ContractId;

/// Purse capability — persistent, consumed and re-produced on Withdraw.
pub const CAP_PURSE: u8 = 0x00;

/// Build the capability descriptor for the Purse contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "purse");
    let purse_cap = CapabilityId::derive(contract_id, CAP_PURSE, b"instance")
        .expect("valid CapabilityId derivation");

    desc.actions = vec![
        // DepositV1 (0x01): Create a Purse with initial balance
        Action {
            function_id: 0x01,
            name: "deposit".into(),
            contract_id,
            description: "Deposit tokens into a Purse — creates a new Purse capability with initial balance".into(),
            requires: CapabilityExpression::Any(vec![]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: purse_cap,
                    description: "Purse capability — consumed and re-produced on Withdraw".into(),
                },
            ],
        },
        // WithdrawV1 (0x02): Withdraw from a Purse (non-terminal — re-produces capability)
        Action {
            function_id: 0x02,
            name: "withdraw".into(),
            contract_id,
            description: "Withdraw tokens from a Purse — proves knowledge of owner_secret, old balance consumed via nullifier, new balance capability produced".into(),
            requires: CapabilityExpression::Any(vec![purse_cap]),
            consumes: vec![purse_cap],
            produces: vec![
                CapabilityOutput {
                    id: purse_cap,
                    description: "Purse capability with reduced balance — re-produced after withdrawal".into(),
                },
            ],
        },
        // BalanceV1 (0x03): Prove knowledge of Purse balance (read-only)
        Action {
            function_id: 0x03,
            name: "balance".into(),
            contract_id,
            description: "Prove knowledge of a Purse's balance without consuming it".into(),
            requires: CapabilityExpression::Any(vec![purse_cap]),
            consumes: vec![],
            produces: vec![],
        },
    ];
    desc
}
