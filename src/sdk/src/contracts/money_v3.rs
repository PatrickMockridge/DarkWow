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

//! Money V3 Contract SDK
//!
//! This module provides the SDK for the Money V3 contract, which handles
//! DeFi tokens (ERC-20 style) with full privacy.
//!
//! ## Token Model
//!
//! MoneyV3 supports MULTIPLE tokens via token authorization.
//! Unlike NativeToken which has a single native token (DARK),
//! MoneyV3 is designed for wrapped tokens, stablecoins, etc.
//!
//! ## Key Differences from NativeToken
//!
//! | Aspect | NativeToken | MoneyV3 |
//! |--------|-------------|---------|
//! | Purpose | Consensus (PoW rewards, fees) | DeFi tokens |
//! | Tokens | Single (DARK) | Multiple (via AuthTokenMint) |
//! | Authorization | None | AuthTokenMint required |
//!
//! ## Contract Functions
//!
//! | Function | Opcode | Purpose |
//! |----------|--------|---------|
//! | TokenMintV1 | 0x00 | Create new token type |
//! | AuthTokenMintV1 | 0x01 | Authorization to mint tokens |
//! | MintV1 | 0x02 | Mint tokens of existing type |
//! | BurnV1 | 0x03 | Burn/destroy tokens |
//! | TransferV1 | 0x04 | Private token transfer |

// Re-export from dwow_money_v3_contract
pub use dwow_money_v3_contract::MoneyV3Function;

// ZK namespaces
pub use dwow_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_TOKEN_MINT_NS_V1;
pub use dwow_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1;
pub use dwow_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_MINT_NS_V1;
pub use dwow_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_BURN_NS_V1;

// Database tree names
pub use dwow_money_v3_contract::MONEY_V3_CONTRACT_COINS_TREE;
pub use dwow_money_v3_contract::MONEY_V3_CONTRACT_NULLIFIERS_TREE;
pub use dwow_money_v3_contract::MONEY_V3_CONTRACT_MERKLE_TREE;
pub use dwow_money_v3_contract::MONEY_V3_CONTRACT_INFO_TREE;

// Constants
pub use dwow_money_v3_contract::MONEY_V3_MAX_COINS_PER_TX;
pub use dwow_money_v3_contract::MONEY_V3_MAX_COIN_VALUE;
pub use dwow_money_v3_contract::MONEY_V3_MIN_COIN_VALUE;
