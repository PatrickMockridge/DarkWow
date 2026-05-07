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

//! 5-Node Local Testnet
//!
//! Phase 3 uncle merkle consensus testing. Extends the 2-node harness
//! pattern to 5 real P2P-connected nodes to verify that:
//! - All 5 nodes reach consensus on the same canonical chain
//! - Blocks with uncle_merkle_root = [0; 32] (Phase 1) are accepted by all nodes
//! - Confirmation threshold is enforced consistently across nodes

pub mod harness;
pub mod tests;

pub use harness::FiveNodeHarness;
