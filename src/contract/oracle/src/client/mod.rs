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

//! Oracle ZK proof client modules

pub mod zkbins;

pub mod register_oracle_v1;
pub mod push_value_v1;
pub mod attest_value_v1;
pub mod push_value_commitment_v1;
pub mod aggregate_v1;

use dwow_sdk::crypto::PublicKey;

use crate::model::SetOracleActiveParamsV1;

/// Builder for setting oracle active state
pub struct SetOracleActiveV1Builder {
    oracle_pub: PublicKey,
    is_active: bool,
}

impl SetOracleActiveV1Builder {
    pub fn new(oracle_pub: PublicKey, is_active: bool) -> Self {
        Self { oracle_pub, is_active }
    }

    pub fn build(self) -> SetOracleActiveParamsV1 {
        SetOracleActiveParamsV1 {
            oracle_pub: self.oracle_pub,
            is_active: self.is_active,
        }
    }
}