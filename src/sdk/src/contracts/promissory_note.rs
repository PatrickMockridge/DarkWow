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

//! Promissory Note Contract SDK
//!
//! This module provides the SDK for the Promissory Note contract, which handles
//! DeFi tokens (ERC-20 style) with full privacy.
//!
//! ## Token Model
//!
//! PromissoryNote supports MULTIPLE tokens via token authorization.
//! Unlike NativeToken which has a single native token (DARK),
//! PromissoryNote is designed for wrapped tokens, stablecoins, etc.
//!
//! ## Key Differences from NativeToken
//!
//! | Aspect | NativeToken | PromissoryNote |
//! |--------|-------------|---------|
//! | Purpose | Consensus (PoW rewards, fees) | DeFi tokens |
//! | Tokens | Single (DARK) | Multiple (via TokenMint) |
//! | Authorization | None | Backing capability proof |
//!
//! ## Contract Functions
//!
//! | Function | Opcode | Purpose |
//! |----------|--------|---------|
//! | TokenMintV1 | 0x00 | Create new token type |
//! | MintV1 | 0x01 | Mint tokens of existing type |
//! | BurnV1 | 0x02 | Burn/destroy tokens |
//! | TransferV1 | 0x03 | Private token transfer |

// Re-export from dwow_promissory_note_contract
pub use dwow_promissory_note_contract::PromissoryNoteFunction;

// ZK namespaces
pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_REGISTER_TYPE_NS_V1;
pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_ISSUE_NS_V1;
pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_REVOKE_NS_V1;
pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_TRANSFER_NS_V1;

// Database tree names
pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_COINS_TREE;
pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE;
pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_MERKLE_TREE;
pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_INFO_TREE;

// Constants
pub use dwow_promissory_note_contract::PROMISSORY_NOTE_MAX_COINS_PER_TX;
pub use dwow_promissory_note_contract::PROMISSORY_NOTE_MAX_COIN_VALUE;
pub use dwow_promissory_note_contract::PROMISSORY_NOTE_MIN_COIN_VALUE;
