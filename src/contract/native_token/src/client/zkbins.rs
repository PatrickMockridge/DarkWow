/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! ZK circuit binary constants for CLIENT-SIDE proof generation.
//!
//! These constants are compiled ONLY when `feature = "client"` is enabled
//! (wallet and test targets). They are NOT compiled into WASM builds.
//!
//! WASM builds use their own `include_bytes!` in `entrypoint/mod.rs` inside
//! `init_contract()` — those are local variables for `zkas_db_set()`, used
//! to store circuits in the on-chain database at deploy time. That is a
//! completely separate code path for a different compilation target.
//!
//! This two-location pattern is inherited from upstream. The two `include_bytes!`
//! sites serve different purposes (client proof building vs on-chain circuit
//! registration) and are compiled into mutually exclusive targets.
//!
//! Usage: the wallet accesses these via the ContractClient trait — never
//! by importing these constants directly.

/// Mint_V1 zkas circuit binary
pub const NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN: &[u8] =
    include_bytes!("../../proof/mint_v1.zk.bin");
/// Burn_V1 zkas circuit binary
pub const NATIVE_TOKEN_CONTRACT_ZKAS_BURN_V1_BIN: &[u8] =
    include_bytes!("../../proof/burn_v1.zk.bin");
/// Fee_V1 zkas circuit binary
pub const NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN: &[u8] =
    include_bytes!("../../proof/fee_v1.zk.bin");
/// FeeCollect_V1 zkas circuit binary
pub const NATIVE_TOKEN_CONTRACT_ZKAS_FEE_COLLECT_V1_BIN: &[u8] =
    include_bytes!("../../proof/fee_collect_v1.zk.bin");

// ── V2 circuits (HAZOP H11: domain separation, M8: coin_public binding) ──
/// Mint_V2 zkas circuit binary
pub const NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V2_BIN: &[u8] =
    include_bytes!("../../proof/mint_v2.zk.bin");
/// Burn_V2 zkas circuit binary
pub const NATIVE_TOKEN_CONTRACT_ZKAS_BURN_V2_BIN: &[u8] =
    include_bytes!("../../proof/burn_v2.zk.bin");
/// Fee_V2 zkas circuit binary
pub const NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V2_BIN: &[u8] =
    include_bytes!("../../proof/fee_v2.zk.bin");
/// FeeCollect_V2 zkas circuit binary
pub const NATIVE_TOKEN_CONTRACT_ZKAS_FEE_COLLECT_V2_BIN: &[u8] =
    include_bytes!("../../proof/fee_collect_v2.zk.bin");
