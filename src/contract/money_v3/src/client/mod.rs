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

//! Money V3 Client API
//!
//! This module implements the client-side API for Money V3 contract interaction.
//!
//! Key design: All commitments use Poseidon hash, no EC operations.
//! This eliminates heap bugs and simplifies the circuit complexity.

use dwow_sdk::{
    crypto::pasta_prelude::PrimeField,
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

// async_trait is required by darkfi-serial derive macros when darkfi-serial/async feature is enabled
#[cfg(feature = "client")]
use dwow_serial::async_trait;

/// `MoneyV3::TokenMintV1` API - create new token type
pub mod token_mint_v1;

/// `MoneyV3::AuthTokenMintV1` API - authorize minting
pub mod auth_token_mint_v1;

/// `MoneyV3::MintV1` API - mint tokens of existing type
pub mod mint_v1;

/// `MoneyV3::BurnV1` API
pub mod burn_v1;

/// `MoneyV3::TransferV1` API
pub mod transfer_v1;

/// MoneyV3Note holds the inner attributes of a Coin.
///
/// Similar to NativeNote but adapted for Money V3's Poseidon-only design.
/// Note that value_blind is pallas::Base (Poseidon commitment), not pallas::Scalar (Pedersen).
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct MoneyV3Note {
    /// Value of the coin
    pub value: u64,
    /// Token ID of the coin
    pub token_id: pallas::Base,
    /// Spend hook used for protocol-owned liquidity
    pub spend_hook: pallas::Base,
    /// User data used by protocol when spend hook is enabled
    pub user_data: pallas::Base,
    /// Blinding factor for the coin
    pub coin_blind: pallas::Base,
    /// Blinding factor for the value (Poseidon hash, not Pedersen)
    pub value_blind: pallas::Base,
    /// Blinding factor for the token ID
    pub token_blind: pallas::Base,
    /// Attached memo (arbitrary data)
    pub memo: Vec<u8>,
}