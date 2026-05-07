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

//! Game Room SDK
//!
//! A software development kit for building game room applications on DarkWow.
//! This SDK provides:
//!
//! - [`types`] - Type definitions for game room structures (rooms, pots, bets)
//! - [`client`] - High-level client for building and signing transactions
//!
//! ## Overview
//!
//! The Game Room contract provides on-chain stake and pot management.
//! Game rules, win conditions, and dispute resolution are handled at the
//! app layer by the room owner (escrow-DAO).
//!
//! App developers use this SDK to:
//! 1. Build transactions for room creation, betting, and claiming
//! 2. Serialize transactions for transmission via DarkIRC
//! 3. Integrate with off-chain game logic
//!
//! ## Usage
//!
//! ```ignore
//! use dwow_sdk::game_room::{GameRoomClient, GameRoomConfig, BetType, EntropyMode};
//! use dwow_sdk::crypto::{Keypair, ContractId};
//!
//! // Create a client
//! let keypair = Keypair::random();
//! let client = GameRoomClient::new(
//!     "http://localhost:8080",
//!     ContractId::derive_public(&keypair.public),
//!     keypair,
//! );
//!
//! // Deposit stake
//! let deposit_tx = client.deposit(room_id, 500);
//!
//! // Place a bet
//! let nonce = client.generate_nonce();
//! let bet_tx = client.place_bet(room_id, 100, BetType::Ante, nonce);
//! ```

pub mod client;
pub mod types;

pub use client::GameRoomClient;
pub use types::*;