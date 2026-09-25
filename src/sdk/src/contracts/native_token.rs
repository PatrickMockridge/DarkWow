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
//! the DRKW token for consensus (block rewards, fees, transfers).
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
//! - Commitment: `poseidon_hash(pub, value, asset_id, spend_hook, user_data, blind)`
//! - Nullifier: `poseidon_hash(spending_key, rho)`
//!
//! ## Contract Functions
//!
//! These are **contract function codes** — the selector byte that begins a call — not zkVM opcodes,
//! which are the 32 instructions in `src/zkas/opcode.rs` that a circuit compiles to. The authoritative
//! list, including each function's plaintext-or-circuit status, is `doc/src/contract/native_token.md`
//! §Function IDs; this table is the ABI summary.
//!
//! | ID | Function | Description |
//! |----|----------|-------------|
//! | 0x00 | — | Unassigned — returns `InvalidFunction` (fee payment is FeeV3 0x08) |
//! | 0x01 | `MintV1` | DISABLED — walled off behind `PoWRewardV1` (consensus-locked coinbase) |
//! | 0x02 | `BurnV1` | Destroy commitments |
//! | 0x03 | `TransferV1` | Private transfers |
//! | 0x04 | `SpendV1` | Spend with change |
//! | 0x05 | `PoWRewardV1` | Coinbase — the block reward |
//! | 0x06 | `FeeCollectV1` | Fee collection — claims the block's fee pot |
//! | 0x07 | `UncleMintV1` | Uncle note mint — spendable uncle reward, no supply bump |
//! | 0x08 | `FeeV3` | Pay fees — plaintext fee + tier (`FeeParamsV3`) |
//!
//! **Plaintext, no proof.** `PoWRewardV1` (0x05), `FeeCollectV1` (0x06) and `UncleMintV1` (0x07) are
//! ordinary consensus calls that carry **no ZK proof and have no circuit**: no `.zk` source exists for
//! any of them, and the WASM entrypoint verifies each in the clear with Pedersen/Poseidon arithmetic.
//! The circuits that once proved them are deleted — `Mint_V2` was removed from the coinbase path,
//! `FeeCollect_V2` was dropped. The only native-token circuits are `Mint_V2` (transfer/spend outputs),
//! `Burn_V2` and `Fee_V3`.

// Re-export from dwow_native_token_contract
pub use dwow_native_token_contract::NativeTokenFunction;

// ZK namespaces (V2 only — V1 circuits deleted, see doc/src/arch/circuit-versioning.md)
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V2;
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V2;
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V3;

// Database tree names
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_COINS_TREE;
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE;
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_INFO_TREE;
pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_FEES_TREE;

// Constants
pub use dwow_native_token_contract::DRKW_ASSET_ID;
pub use dwow_native_token_contract::NATIVE_TOKEN_MAX_COINS_PER_TX;
pub use dwow_native_token_contract::NATIVE_TOKEN_MAX_COIN_VALUE;
pub use dwow_native_token_contract::NATIVE_TOKEN_MIN_COIN_VALUE;
