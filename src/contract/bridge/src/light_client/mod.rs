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

//! Light Client Module
//!
//! Provides trustless verification of external chain state via SPV.
//!
//! ## Why Light Clients?
//!
//! Traditional bridges use oracles to verify deposits, but this just
//! moves trust from the bridge to the oracle. Light clients provide
//! trustless verification by verifying proofs against block headers.
//!
//! ## Architecture
//!
//! Each external chain has a `LightClient` implementation that:
//! - Fetches block headers via RPC
//! - Verifies Merkle proofs
//! - Tracks chain reorganizations
//! - Provides finality guarantees
//!
//! ## Implementations
//!
//! - `EthLightClient`: Ethereum SPV via JSON-RPC
//! - `BtcLightClient`: Bitcoin SPV via JSON-RPC
//! - `XmrLightClient`: Monero view-key scanning
//! - `ZecLightClient`: Zcash lightwalletd RPC

pub mod client;

pub use client::{
    BlockHeader, ConfirmationLevel, Error, FinalityProof, LightClient, MerkleProof,
};
