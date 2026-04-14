/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation, either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! Clean sync module for DarkFi node.
//!
//! Design intent:
//! - Blockchain sync: Receive block → Verify → Apply
//! - VKs derived at verification time, never stored
//! - No overlay caching during sync
//! - Modular: each piece testable independently

pub mod apply;
pub mod types;
pub mod verify;

pub use apply::apply_block;
pub use types::{SyncBlock, SyncState, VerifyResult};
pub use verify::{verify_block, verify_header};