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

//! NativeToken Client API
//!
//! This module implements the client-side API for NativeToken contract interaction.

use dwow_sdk::pasta::pallas;
use dwow_serial::{SerialDecodable, SerialEncodable};

/// `NativeToken::BurnV1` API
pub mod burn_v1;

/// `NativeToken::FeeV1` API
pub mod fee_v1;

/// `NativeToken::PoWRewardV1` API
pub mod pow_reward_v1;

/// `NativeToken::TransferV1` API
pub mod transfer_v1;

/// NativeNote holds the inner attributes of a Coin.
///
/// It does not store the public key since it's encrypted for that key,
/// and so is not needed to infer the coin attributes.
/// All other coin attributes must be present.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct NativeNote {
    /// Value of the coin
    pub value: u64,
    /// Token ID of the coin
    pub token_id: pallas::Base,
    /// Spend hook used for protocol-owned liquidity.
    pub spend_hook: pallas::Base,
    /// User data used by protocol when spend hook is enabled
    pub user_data: pallas::Base,
    /// Blinding factor for the coin
    pub coin_blind: pallas::Base,
    /// Blinding factor for the value pedersen commitment
    pub value_blind: pallas::Scalar,
    /// Blinding factor for the token ID pedersen commitment
    pub token_blind: pallas::Base,
    /// Attached memo (arbitrary data)
    pub memo: Vec<u8>,
}