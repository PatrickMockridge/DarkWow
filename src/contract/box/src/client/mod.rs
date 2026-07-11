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

//! Box contract client — wallet-side capability construction.
//!
//! Provides parameter construction for PutV1 and TakeV1, and the
//! ContractClient impl for the wallet's generic dispatch.

/// ZK circuit binary constants for client-side proof generation.
pub mod zkbins;

use dwow_sdk::contract_client::{ContractClient, WalletStateProvider};

use crate::model::{PutParamsV1, TakeParamsV1};

/// Box contract client.
pub struct BoxClient;

impl ContractClient for BoxClient {
    fn contract_name(&self) -> &'static str {
        "box"
    }

    fn function_selector(&self, function: &str) -> Option<u8> {
        match function {
            "PutV1" => Some(0x01),
            "TakeV1" => Some(0x02),
            _ => None,
        }
    }

    fn supported_functions(&self) -> Vec<&'static str> {
        vec!["PutV1", "TakeV1"]
    }

    fn build(
        &self,
        function: &str,
        _params: &str,
        _wallet_state: &dyn WalletStateProvider,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        match function {
            "PutV1" | "TakeV1" => Ok((vec![], vec![])),
            _ => Err(format!("Box: unsupported function '{}'", function)),
        }
    }
}
