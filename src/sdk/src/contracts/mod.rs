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

//! Contract SDKs for DarkWow
//!
//! This module provides clean interfaces to contract client APIs.
//! Each contract SDK exports:
//! - Function opcodes (for building calldata)
//! - ZK namespaces (for proof generation)
//! - Token ID constants
//! - Database tree names
//!
//! ## Available SDKs
//!
//! | Contract | Module | Status |
//! |----------|--------|--------|
//! | Native Token | [`native_token`] | ✅ Working |
//! | Money V3 | [`money_v3`] | ✅ Working |
//! | DAO Escrow | [`dao_escrow`] | ⚠️ Broken |

pub mod native_token;
pub mod money_v3;
// pub mod dao_escrow; // TODO: Fix dao_escrow contract bugs
