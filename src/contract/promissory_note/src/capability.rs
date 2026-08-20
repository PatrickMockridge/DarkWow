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

//! Capability descriptor for the Promissory Note contract.
//!
//! Promissory Note is both a contract AND the capability source for all coins.
//! Every unspent coin is a capability ("I can spend this coin"). Mint authorities
//! are capabilities ("I can mint tokens of this type"). Receipt coins are
//! non-consumable capabilities proving redemption.
//!
//! ## Capability Types
//!
//! | Type | Discriminant | Source | Consumable |
//! |------|-------------|--------|------------|
//! | Spendable Coin | 0x00 | Unspent coin in wallet | Yes |
//! | Mint Authority | 0x01 | Knows mint_secret for asset_id | No |
//! | Receipt Coin | 0x02 | Unspent coin with value=0 | No |
//!
//! The capability ID for a coin capability is derived as:
//! `CapabilityId::derive(contract_id, capability_type, coin_id_bytes)`
//!
//! For mint authorities:
//! `CapabilityId::derive(contract_id, CAP_MINT_AUTHORITY, asset_id_bytes)`

use dwow_sdk::capability::{
    Action, CapabilityDescriptor, CapabilityExpression, CapabilityId, CapabilityOutput,
};
use dwow_sdk::crypto::ContractId;

/// Spendable coin capability
pub const CAP_NOTE: u8 = 0x00;
/// Mint authority capability (knows backing secret for a token)
pub const CAP_MINT_AUTHORITY: u8 = 0x01;
/// Receipt coin capability (redemption proof, non-transferable)
pub const CAP_RECEIPT: u8 = 0x02;

/// Build the capability descriptor for the Promissory Note contract.
pub fn descriptor(contract_id: ContractId) -> CapabilityDescriptor {
    let mut desc = CapabilityDescriptor::new(contract_id, "promissory_note");
    desc.actions = vec![
        // TransferV1 (0x04): Private transfer — burn + blind output
        Action {
            function_id: 0x04,
            name: "TransferV1".into(),
            contract_id,
            description: "Transfer capabilities to a recipient (atomic burn + mint)".into(),
            requires: CapabilityExpression::Any(vec![
                CapabilityId::derive(contract_id, CAP_NOTE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_NOTE, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_NOTE, b"output").expect("valid CapabilityId derivation"),
                    description: "New capability for recipient".into(),
                },
            ],
        },
        // RevokeV1 (0x03): Destroy coins, publish nullifiers
        Action {
            function_id: 0x03,
            name: "RevokeV1".into(),
            contract_id,
            description: "Revoke capabilities — destroy value, publish nullifiers".into(),
            requires: CapabilityExpression::Any(vec![
                CapabilityId::derive(contract_id, CAP_NOTE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_NOTE, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![],
        },
        // RedeemV1 (0x01): Redeem coin, create receipt
        Action {
            function_id: 0x01,
            name: "RedeemV1".into(),
            contract_id,
            description: "Redeem a capability with the issuer — creates a zero-value receipt".into(),
            requires: CapabilityExpression::Any(vec![
                CapabilityId::derive(contract_id, CAP_NOTE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_NOTE, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_RECEIPT, b"receipt").expect("valid CapabilityId derivation"),
                    description: "Receipt coin — proof of redemption".into(),
                },
            ],
        },
        // IssueV1 (0x02): Mint tokens of existing type
        Action {
            function_id: 0x02,
            name: "IssueV1".into(),
            contract_id,
            description: "Issue new capabilities of an existing token type".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_MINT_AUTHORITY, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_NOTE, b"output").expect("valid CapabilityId derivation"),
                    description: "Newly minted coin".into(),
                },
            ],
        },
        // RegisterTypeV1 (0x00): Create a new token type
        Action {
            function_id: 0x00,
            name: "RegisterTypeV1".into(),
            contract_id,
            description: "Create a new token type with backing capability".into(),
            requires: CapabilityExpression::All(vec![
                CapabilityId::derive(contract_id, CAP_MINT_AUTHORITY, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_NOTE, b"output").expect("valid CapabilityId derivation"),
                    description: "Initial minted coin".into(),
                },
            ],
        },
        // OtcSwapV1 (0x05): Atomic OTC swap
        Action {
            function_id: 0x05,
            name: "OtcSwapV1".into(),
            contract_id,
            description: "Atomic peer-to-peer token swap".into(),
            requires: CapabilityExpression::Any(vec![
                CapabilityId::derive(contract_id, CAP_NOTE, b"instance").expect("valid CapabilityId derivation"),
            ]),
            consumes: vec![
                CapabilityId::derive(contract_id, CAP_NOTE, b"instance").expect("valid CapabilityId derivation"),
            ],
            produces: vec![
                CapabilityOutput {
                    id: CapabilityId::derive(contract_id, CAP_NOTE, b"output").expect("valid CapabilityId derivation"),
                    description: "Swapped coin from counterparty".into(),
                },
            ],
        },
    ];
    desc
}
