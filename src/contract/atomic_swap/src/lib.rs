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

//! DarkWow Atomic Swap Contract
//!
//! Cross-chain atomic swaps via Hashed Timelock Contract (HTLC) pattern.
//!
//! ## How It Works
//!
//! ```
//! Chain A (e.g., Ethereum)              Chain B (DarkWow)
//! ─────────────────────────────────   ─────────────────────────────────
//! 1. Alice locks funds in HTLC         1. Bob locks funds in HTLC
//!     hash = SHA256(secret)                 hash = SHA256(secret)
//!     timelock = block + N                  timelock = block + N
//!                                            (wait for Alice)
//! 2. Alice reveals secret           ──────────────────────────────►
//!     (on-chain)                         2. Bob sees secret
//!                                            3. Bob claims on DarkWow
//!                                            4. Secret revealed
//! 5. Alice claims on Ethereum      ◄────────────────────────────────
//! ```
//!
//! If timelock expires:
//! - Alice can refund on Chain A
//! - Bob can refund on DarkWow
//!
//! ## Security Properties
//!
//! - **Atomic**: Either both sides complete, or neither
//! - **Hashlock**: Only holder of secret can claim
//! - **Timelock**: Refund guaranteed after expiration
//! - **Non-custodial**: Neither party holds other's funds
//!
//! ## Cross-Chain Integration
//!
//! This contract handles the DarkWow side of the swap. The external chain
//! (Ethereum, Bitcoin, etc.) implements its own HTLC with the same hash.

use darkfi_sdk::define_contract_function;

define_contract_function!(AtomicSwapFunction {
    InitializeV1 = 0x00,
    CreateSwapV1 = 0x01,
    ClaimV1 = 0x02,
    RefundV1 = 0x03,
});

/// Call parameters definitions
pub mod model;

/// Error types
pub mod error;

/// Client API
#[cfg(feature = "client")]
pub mod client;

/// WASM entrypoint functions
#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Info tree (version, config)
pub const ATOMIC_SWAP_CONTRACT_INFO_TREE: &str = "info";
/// Swaps tree (active swaps)
pub const ATOMIC_SWAP_CONTRACT_SWAPS_TREE: &str = "swaps";
/// Secrets tree (revealed secrets, cleared after use)
pub const ATOMIC_SWAP_CONTRACT_SECRETS_TREE: &str = "secrets";
/// Nullifiers tree (prevents double-claim/refund)
pub const ATOMIC_SWAP_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";

// ============================================================================
// DATABASE KEYS
// ============================================================================

pub const ATOMIC_SWAP_CONTRACT_DB_VERSION: &[u8] = b"db_version";

// ============================================================================
// zkas CIRCUIT NAMESPACES
// ============================================================================

/// CreateSwap circuit namespace
pub const ATOMIC_SWAP_CONTRACT_ZKAS_CREATE_NS: &str = "CreateSwap_V1";
/// Claim circuit namespace
pub const ATOMIC_SWAP_CONTRACT_ZKAS_CLAIM_NS: &str = "ClaimSwap_V1";
/// Refund circuit namespace
pub const ATOMIC_SWAP_CONTRACT_ZKAS_REFUND_NS: &str = "RefundSwap_V1";