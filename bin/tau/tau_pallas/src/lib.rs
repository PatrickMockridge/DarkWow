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

//! Tau_Pallas: Pallas-native tau with DarkFi on-chain integration
//!
//! This is a variant of tau that uses darkfi_sdk Pallas-native crypto
//! throughout, enabling direct transaction signing and darkfid RPC integration
//! for on-chain capability verification.

pub mod capability;
pub mod error;
pub mod identity_client;
pub mod month_tasks;
pub mod rpc_client;
pub mod task_info;
pub mod util;
