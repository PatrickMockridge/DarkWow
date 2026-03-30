/* This file is part of DarkFi (https://dark.fi)
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

// UNUSED: async_trait is imported but not used - async serialization is handled
// by darkfi-serial derive macros when async feature is enabled (via darkfi/validator).
// This import is dead code and can be removed if darkfi-serial/async is ever fixed.
#[cfg(feature = "client")]
// use darkfi_serial::async_trait;

use darkfi_sdk::crypto::{ContractId, PublicKey};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// State update for `Deploy::Deploy`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct DeployUpdateV1 {
    /// The `ContractId` to deploy
    pub contract_id: ContractId,
}

// ANCHOR: deploy-lock-params
/// Parameters for `Deploy::Lock`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct LockParamsV1 {
    /// Public key used to sign the transaction and derive the `ContractId`
    pub public_key: PublicKey,
}
// ANCHOR_END: deploy-lock-params

/// State update for `Deploy::Lock`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct LockUpdateV1 {
    /// The `ContractId` to lock
    pub contract_id: ContractId,
}
