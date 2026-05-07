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

//! Clean sync module for DarkWow node.
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
pub use types::{SyncBlock, SyncState, VerifyResult, ZkBinEntry};
pub use verify::{verify_block, verify_header};