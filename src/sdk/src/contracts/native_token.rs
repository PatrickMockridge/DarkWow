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

//! Native Token Contract SDK
//!
//! This module provides the SDK for the Native Token contract, which handles
//! the DARK token for consensus (block rewards, fees, transfers).
//!
//! ## Design Philosophy
//!
//! CONSENSUS FIRST, FEES SECOND, PRIVACY THIRD
//!
//! The native token contract serves as the native token for DarkWow with:
//! 1. **Consensus Reward** - Block rewards for PoW mining must be reliable
//! 2. **Network Fees** - Transaction fee payment must be deterministic
//! 3. **Privacy Layer** - Privacy on top, never compromising consensus
//!
//! ## Token Model
//!
//! Uses Poseidon commitments (no EC = no heap bugs):
//! - Coin: `poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)`
//! - Nullifier: `poseidon_hash(spending_key, rho)`
//!
//! ## Contract Functions
//!
//! | Function | Opcode | Purpose |
//! |----------|--------|---------|
//! | FeeV1 | 0x00 | Pay network fees |
//! | MintV1 | 0x01 | DISABLED — walled off behind PoWRewardV1 (consensus-locked coinbase) |
//! | BurnV1 | 0x02 | Destroy coins |
//! | TransferV1 | 0x03 | Private transfers |
//! | SpendV1 | 0x04 | Spend with change |
//! | PoWRewardV1 | 0x05 | Block rewards for miners |

// Re-export from dwow_native_token_contract
pub use dwow_native_token_contract::NativeTokenFunction;

// ZK namespaces
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1;
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V1;
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V1;

// Database tree names
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_COINS_TREE;
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE;
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_MERKLE_TREE;
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_INFO_TREE;
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_FEES_TREE;

// Constants
pub use dwow_native_token_contract::DRKW_TOKEN_ID;
pub use dwow_native_token_contract::NATIVE_TOKEN_MAX_COINS_PER_TX;
pub use dwow_native_token_contract::NATIVE_TOKEN_MAX_COIN_VALUE;
pub use dwow_native_token_contract::NATIVE_TOKEN_MIN_COIN_VALUE;
