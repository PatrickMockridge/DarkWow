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

//! 2-Node Base Harness for linear blockchain sync tests.
//!
//! Creates two GenesisHarness instances (alice and bob) for testing
//! block sync and contract propagation between independent nodes.

use dwow::Result;

use super::genesis::GenesisHarness;

/// Two independent linear blockchain nodes for sync testing.
pub struct Harness {
    pub alice: GenesisHarness,
    pub bob: GenesisHarness,
}

impl Harness {
    /// Create two independent GenesisHarness instances.
    pub fn new() -> Result<Self> {
        Ok(Self { alice: GenesisHarness::new()?, bob: GenesisHarness::new()? })
    }
}
