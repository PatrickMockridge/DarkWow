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

/// Access control for host functions
mod acl;

/// Host functions for interacting with db backend
pub(crate) mod db;

/// Host functions for merkle tree functions
pub(crate) mod merkle;

/// Host functions for block-level merkle tree anchoring (§C.3.7)
pub(crate) mod merkle_anchor;

/// Host functions for sparse merkle tree functions
pub(crate) mod smt;

/// Host functions for cross-shard proofs (post-mainnet scaffolding).
/// See doc/src/arch/consensus/scaling.md.
#[cfg(feature = "sharding")]
pub(crate) mod shard;

/// Host functions for utilities
pub(crate) mod util;
